use clap::Parser;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(about = "Check web endpoints against expectations using a headless browser")]
pub struct Cli {
    /// Base host to check (e.g. "localhost", "staging.mdrepo.org", "mdrepo.org")
    #[arg(short, long, default_value = "localhost")]
    pub url: String,

    /// Port (e.g. 8080); if omitted, no port is added to the URL
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Force HTTP instead of HTTPS (default: HTTPS for non-localhost hosts)
    #[arg(long)]
    pub http: bool,

    /// Path to TOML config file listing endpoints to check
    #[arg(short, long, default_value = "endpoints.toml")]
    pub config: String,

    /// Request timeout in seconds
    #[arg(short, long, default_value = "30")]
    pub timeout: u64,

    /// Show response details even for passing checks
    #[arg(short, long)]
    pub verbose: bool,

    /// Run with a visible browser window (useful for debugging)
    #[arg(long)]
    pub headed: bool,

    /// Path to Chrome/Chromium binary (auto-detected if not set)
    #[arg(long)]
    pub chrome: Option<String>,
}

/// One endpoint definition from the config file.
#[derive(Debug, Deserialize)]
pub struct Endpoint {
    /// URL path, e.g. "/simulations"
    pub path: String,

    /// If set, only check this endpoint when --url matches this host exactly
    pub host: Option<String>,

    /// Expected HTTP status code (default: 200)
    #[serde(default = "default_status")]
    pub expected_status: u16,

    /// Use a headless browser (default: true). Set to false for API/JSON endpoints.
    #[serde(default = "default_true")]
    pub browser: bool,

    /// Strings that must appear in the rendered page content
    #[serde(default)]
    pub contains: Vec<String>,

    /// Strings that must NOT appear in the rendered page content
    #[serde(default)]
    pub not_contains: Vec<String>,

    /// JSON field checks (only meaningful when browser = false)
    #[serde(default)]
    pub json_checks: Vec<JsonCheck>,
}

/// A check against a specific field in a JSON response body.
#[derive(Debug, Deserialize)]
pub struct JsonCheck {
    /// Dot-notation + bracket-index path, e.g. "results[0].title"
    pub path: String,

    /// Field must equal this value (also accepted as "eq")
    #[serde(alias = "eq")]
    pub equals: Option<serde_json::Value>,

    /// String field must contain this substring
    pub contains: Option<String>,

    /// Numeric field must be greater than this
    pub gt: Option<f64>,

    /// Numeric field must be less than this
    pub lt: Option<f64>,

    /// Field must exist (default: true)
    #[serde(default = "default_true")]
    pub exists: bool,
}

fn default_status() -> u16 {
    200
}

fn default_true() -> bool {
    true
}

/// Top-level shape of the TOML config file.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub endpoints: Vec<Endpoint>,
}

/// Outcome of checking one endpoint.
#[derive(Debug)]
pub struct CheckResult {
    pub path: String,
    pub expected_status: u16,
    pub actual_status: Option<u16>,
    pub elapsed_ms: u128,
    pub failures: Vec<String>,
    pub error: Option<String>,
}

impl CheckResult {
    pub fn passed(&self) -> bool {
        self.error.is_none() && self.failures.is_empty()
    }
}
