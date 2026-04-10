use crate::{
    process,
    types::{ProcessArgs, TicketArgs, TicketInfo},
};
use anyhow::{anyhow, bail, Result};
use dotenvy::dotenv;
use log::{debug, info};
use std::{env, fs, path::PathBuf, process::Command};
use which::which;

// --------------------------------------------------
pub fn get_ticket_user(args: &TicketArgs) -> Result<TicketInfo> {
    let work_dir = &args.work_dir.clone().unwrap_or(PathBuf::from(
        env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
    ));

    let ticket_file = work_dir
        .join("landing")
        .join(args.server.to_string())
        .join(format!("ticket-{}", args.ticket_id))
        .join("ticket.json");

    let contents = fs::read_to_string(&ticket_file)
        .map_err(|e| anyhow!("{}: {e}", ticket_file.display()))?;

    let ticket: TicketInfo = serde_json::from_str(&contents)?;

    Ok(ticket)
}

// --------------------------------------------------
pub fn process(args: &TicketArgs) -> Result<()> {
    debug!("{args:?}");
    dotenv().ok();

    let script_dir = &args.script_dir.clone().unwrap_or(PathBuf::from(
        env::var("SCRIPT_DIR").map_err(|e| anyhow!("SCRIPT_DIR: {e}"))?,
    ));
    let work_dir = args.work_dir.clone().unwrap_or(PathBuf::from(
        env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
    ));
    let landing_dir = &work_dir.join("landing");
    let landing_dir = &landing_dir.join(args.server.to_string());
    if !landing_dir.is_dir() {
        fs::create_dir_all(landing_dir)?;
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
        .current_dir(script_dir)
        .args([
            "run",
            fetch.to_string_lossy().as_ref(),
            "--server",
            &args.server.to_string(),
            "--ticket-id",
            &args.ticket_id.to_string(),
            "--landing-dir",
            landing_dir.to_string_lossy().as_ref(),
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

    let mut ticket_dirs = vec![];
    for entry in fs::read_dir(ticket_dir)? {
        let entry = entry?;
        let entry = entry.path();
        if entry.is_dir() {
            ticket_dirs.push(entry);
        }
    }

    if ticket_dirs.is_empty() {
        bail!("Failed to download any directories for ticket")
    }

    for ticket_dir in ticket_dirs {
        debug!(r#"Processing ticket directory "{}""#, ticket_dir.display());
        match process::process(&ProcessArgs {
            input_dir: ticket_dir.clone(),
            script_dir: Some(script_dir.clone()),
            work_dir: Some(work_dir.clone()),
            out_dir: None,
            server: args.server.clone(),
            reprocess_simulation_id: None,
            // The TOML will have already been validated, so allow missing IDs
            no_id: true,
            force: args.force,
            dry_run: args.dry_run,
        }) {
            Ok(()) => {
                info!(
                    r#"Finished processing ticket directory "{}""#,
                    ticket_dir.display()
                );
            }
            Err(e) => {
                info!("{e}");
                bail!(
                    r#"Error processing ticket directory "{}""#,
                    ticket_dir.display()
                )
            }
        }
    }

    info!(r#"Done processing ticket "{}""#, args.ticket_id);

    Ok(())
}
