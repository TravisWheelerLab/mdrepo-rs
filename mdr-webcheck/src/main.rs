mod json;
mod types;

use anyhow::{Context, Result};
use chromiumoxide::{Browser, BrowserConfig};
use clap::Parser;
use futures::StreamExt;
use reqwest::Client;
use std::{fs, time::Instant};
use types::{CheckResult, Cli, Config, Endpoint};

// --------------------------------------------------
#[tokio::main]
async fn main() {
    if let Err(e) = run(Cli::parse()).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
async fn run(args: Cli) -> Result<()> {
    let raw = fs::read_to_string(&args.config)
        .with_context(|| format!("could not read config file \"{}\"", args.config))?;
    let config: Config = toml::from_str(&raw)
        .with_context(|| format!("could not parse config file \"{}\"", args.config))?;

    let base_url = build_base_url(&args);

    // Keep only endpoints that apply to the current host and match any filter.
    let filter = args.filter.as_deref().map(str::to_lowercase);
    let mut endpoints: Vec<Endpoint> = config
        .endpoints
        .into_iter()
        .filter(|e| e.host.as_deref().map_or(true, |h| h == args.url))
        .filter(|e| {
            filter
                .as_deref()
                .map_or(true, |f| e.path.to_lowercase().contains(f))
        })
        .collect();

    println!("Checking {} paths at {base_url} ...\n", endpoints.len());

    // For API checks, bake the admin token into each path so it's sent as a query param.
    if let Some(token) = &args.admin_token {
        for endpoint in endpoints.iter_mut() {
            if !endpoint.browser {
                endpoint.path = if endpoint.path.contains('?') {
                    format!("{}&admin_token={}", endpoint.path, token)
                } else {
                    format!("{}?admin_token={}", endpoint.path, token)
                };
            }
        }
    }

    let needs_browser = endpoints.iter().any(|e| e.browser);
    let browser_opt = if needs_browser {
        // Use a per-process directory so concurrent or crashed runs never
        // leave a stale SingletonLock that blocks the next launch.
        let user_data_dir = std::env::temp_dir()
            .join(format!("mdr-webcheck-{}", std::process::id()));
        let mut builder = BrowserConfig::builder()
            .user_data_dir(user_data_dir);
        let chrome_path = args.chrome.clone().or_else(detect_chrome).ok_or_else(|| {
            anyhow::anyhow!(
                "No ARM-compatible Chrome/Chromium found. \
                 Install Google Chrome or specify --chrome <path>."
            )
        })?;
        builder = builder.chrome_executable(chrome_path);
        let (browser, mut handler) = Browser::launch(
            builder.build().map_err(|e| anyhow::anyhow!(e))?,
        )
        .await?;
        tokio::spawn(async move { while handler.next().await.is_some() {} });
        // Log in via the admin token so the browser session cookie is set for
        // all subsequent browser checks.
        if let Some(token) = &args.admin_token {
            browser_admin_login(&browser, &base_url, token).await?;
        }

        Some(browser)
    } else {
        None
    };

    let http_client = Client::builder()
        .timeout(std::time::Duration::from_secs(args.timeout))
        .danger_accept_invalid_certs(
            args.url.contains("localhost") || args.url.contains("127.0.0.1"),
        )
        .build()?;

    let mut passed = 0usize;
    let mut failed = 0usize;
    let total = endpoints.len();

    for endpoint in endpoints {
        println!("  checking {} ...", endpoint.path);

        let result = if endpoint.browser {
            let browser = browser_opt.as_ref().expect("browser was initialised above");
            check_browser(browser, &base_url, endpoint, args.timeout).await
        } else {
            check_http(&http_client, &base_url, endpoint).await
        };

        print_result(&result, args.verbose);
        if result.passed() {
            passed += 1;
        } else {
            failed += 1;
        }
    }

    println!("\n{total} checked, {passed} passed, {failed} failed");

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

// --------------------------------------------------
async fn browser_admin_login(
    browser: &Browser,
    base_url: &str,
    token: &str,
) -> Result<()> {
    let url = format!("{}/api/v1/admin_login?admin_token={}", base_url, token);
    let page = browser.new_page(&url).await?;
    let status = page
        .evaluate(
            "performance.getEntriesByType('navigation')[0]?.responseStatus ?? 0",
        )
        .await?
        .into_value::<f64>()
        .unwrap_or(0.0) as u16;
    if status != 200 {
        anyhow::bail!("admin login failed with HTTP {status} — check ADMIN_SECRET_KEY");
    }
    println!("  admin login OK (session cookie set)\n");
    Ok(())
}

// --------------------------------------------------
async fn check_browser(
    browser: &Browser,
    base_url: &str,
    endpoint: Endpoint,
    timeout_secs: u64,
) -> CheckResult {
    let url = format!("{}{}", base_url, endpoint.path);
    let start = Instant::now();

    match check_browser_inner(browser, &url, &endpoint, timeout_secs).await {
        Ok((actual_status, failures)) => CheckResult {
            path: endpoint.path,
            expected_status: endpoint.expected_status,
            actual_status: Some(actual_status),
            elapsed_ms: start.elapsed().as_millis(),
            failures,
            error: None,
        },
        Err(e) => CheckResult {
            path: endpoint.path,
            expected_status: endpoint.expected_status,
            actual_status: None,
            elapsed_ms: start.elapsed().as_millis(),
            failures: vec![],
            error: Some(e.to_string()),
        },
    }
}

// --------------------------------------------------
async fn check_browser_inner(
    browser: &Browser,
    url: &str,
    endpoint: &Endpoint,
    _timeout_secs: u64,
) -> Result<(u16, Vec<String>)> {
    let page = browser.new_page(url).await?;

    let actual_status = page
        .evaluate(
            "performance.getEntriesByType('navigation')[0]?.responseStatus ?? 0",
        )
        .await?
        .into_value::<f64>()
        .unwrap_or(0.0) as u16;

    let content = page.content().await?;
    let mut failures = vec![];

    if actual_status != endpoint.expected_status {
        failures.push(format!(
            "expected status {}, got {actual_status}",
            endpoint.expected_status
        ));
    }
    check_content(&content, &endpoint.contains, &endpoint.not_contains, &mut failures);

    Ok((actual_status, failures))
}

// --------------------------------------------------
async fn check_http(
    client: &Client,
    base_url: &str,
    endpoint: Endpoint,
) -> CheckResult {
    let url = format!("{}{}", base_url, endpoint.path);
    let start = Instant::now();

    match check_http_inner(client, &url, &endpoint).await {
        Ok((actual_status, failures)) => CheckResult {
            path: endpoint.path,
            expected_status: endpoint.expected_status,
            actual_status: Some(actual_status),
            elapsed_ms: start.elapsed().as_millis(),
            failures,
            error: None,
        },
        Err(e) => CheckResult {
            path: endpoint.path,
            expected_status: endpoint.expected_status,
            actual_status: None,
            elapsed_ms: start.elapsed().as_millis(),
            failures: vec![],
            error: Some(e.to_string()),
        },
    }
}

// --------------------------------------------------
async fn check_http_inner(
    client: &Client,
    url: &str,
    endpoint: &Endpoint,
) -> Result<(u16, Vec<String>)> {
    let resp = client.get(url).send().await?;
    let actual_status = resp.status().as_u16();

    let needs_body = !endpoint.contains.is_empty()
        || !endpoint.not_contains.is_empty()
        || !endpoint.json_checks.is_empty();
    let body = if needs_body {
        resp.text().await.unwrap_or_default()
    } else {
        String::new()
    };

    let mut failures = vec![];
    if actual_status != endpoint.expected_status {
        failures.push(format!(
            "expected status {}, got {actual_status}",
            endpoint.expected_status
        ));
    }
    check_content(&body, &endpoint.contains, &endpoint.not_contains, &mut failures);
    json::apply_json_checks(&body, &endpoint.json_checks, None, &mut failures);

    Ok((actual_status, failures))
}

// --------------------------------------------------
fn check_content(
    body: &str,
    contains: &[String],
    not_contains: &[String],
    failures: &mut Vec<String>,
) {
    for needle in contains {
        if !body.contains(needle.as_str()) {
            failures.push(format!("body missing {:?}", needle));
        }
    }
    for needle in not_contains {
        if body.contains(needle.as_str()) {
            failures.push(format!("body unexpectedly contains {:?}", needle));
        }
    }
}

// --------------------------------------------------
fn detect_chrome() -> Option<String> {
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists() && is_arm_compatible(p))
        .map(|p| p.to_string())
}

// --------------------------------------------------
// On macOS/aarch64, reject binaries that don't include an arm64 slice.
// On all other platforms this is a no-op and always returns true.
fn is_arm_compatible(path: &str) -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        std::process::Command::new("lipo")
            .args(["-archs", path])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("arm64"))
            .unwrap_or(false)
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = path;
        true
    }
}

// --------------------------------------------------
fn build_base_url(args: &Cli) -> String {
    let is_local =
        args.url.contains("localhost") || args.url.contains("127.0.0.1");
    let scheme = if args.http || is_local { "http" } else { "https" };
    let host = args
        .url
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    match args.port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

// --------------------------------------------------
fn print_result(result: &CheckResult, verbose: bool) {
    let status_str = match result.actual_status {
        Some(s) => s.to_string(),
        None => "---".to_string(),
    };
    let time_str = format!("{}ms", result.elapsed_ms);

    if result.passed() {
        println!("  PASS  {:<40} {:<6} {}", result.path, status_str, time_str);
        return;
    }

    println!("  FAIL  {:<40} {:<6} {}", result.path, status_str, time_str);
    if let Some(err) = &result.error {
        println!("        error: {err}");
    }
    for failure in &result.failures {
        println!("        {failure}");
    }
    if verbose {
        println!("        (expected status {})", result.expected_status);
    }
}
