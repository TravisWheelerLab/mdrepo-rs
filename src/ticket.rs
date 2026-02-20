use crate::{
    process,
    types::{ProcessArgs, TicketArgs},
};
use anyhow::{anyhow, bail, Result};
use log::{debug, info};
use std::{env, fs, path::PathBuf, process::Command};
use which::which;

// --------------------------------------------------
pub fn process(args: &TicketArgs) -> Result<()> {
    debug!("{args:?}");
    dotenv::dotenv()?;
    let script_dir = &args
        .script_dir
        .clone()
        .unwrap_or(PathBuf::from(env::var("SCRIPT_DIR")?));

    let landing_dir = &args
        .landing_dir
        .clone()
        .unwrap_or(PathBuf::from(env::var("LANDING_DIR")?));
    let landing_dir = &landing_dir.join(&args.server.to_string());
    if !landing_dir.is_dir() {
        fs::create_dir_all(&landing_dir)?;
    }

    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    let fetch = script_dir.join("fetch_uploads.py");
    let ticket_dir = &landing_dir.join(format!("ticket-{}", args.ticket_id));

    info!(
        r#"Fetching ticket "{}" -> "{}""#,
        args.ticket_id,
        ticket_dir.display()
    );

    let cmd = Command::new(&uv)
        .current_dir(&script_dir)
        .args([
            "run",
            &fetch.to_string_lossy().to_string(),
            "--server",
            &args.server.to_string(),
            "--ticket-id",
            &args.ticket_id.to_string(),
            "--landing-dir",
            &landing_dir.to_string_lossy().to_string(),
        ])
        .output()?;

    if !cmd.status.success() {
        bail!(str::from_utf8(&cmd.stderr)?.to_string());
    }

    // The ticket directory should have been created by the fetch
    if !ticket_dir.is_dir() {
        bail!(
            r#"Failed to create ticket directory "{}""#,
            ticket_dir.display()
        );
    }

    for entry in fs::read_dir(&ticket_dir)? {
        let entry = entry?;
        let dirname = entry.path();
        if dirname.is_dir() {
            debug!(r#"Process "{}""#, dirname.display());
            match process::process(&ProcessArgs {
                dirname,
                script_dir: Some(script_dir.clone()),
                out_dir: None,
                json_dir: None,
                server: args.server.clone(),
            }) {
                Ok(()) => info!("Success"),
                Err(e) => info!("Error: {e}"),
            }
        }
    }

    Ok(())
}
