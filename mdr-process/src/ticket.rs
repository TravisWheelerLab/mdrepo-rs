use crate::{
    process,
    types::{ProcessArgs, Server, TicketArgs, TicketInfo},
};
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use diesel::pg::PgConnection;
use dotenvy::dotenv;
//use libmdrepo::metadata::Meta;
use log::{debug, info};
use mdr_db::{
    models::{
        NewUploadInstance, NewUploadMessage, TicketUpdate, UploadInstanceUpdate,
    },
    ops,
};
use rayon::prelude::*;
use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
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

    let ticket_dir = &landing_dir.join(format!("ticket-{}", args.ticket_id));
    debug!(
        r#"Processing ticket "{}" -> "{}""#,
        args.ticket_id,
        ticket_dir.display()
    );

    if args.skip_download {
        debug!("Skipping download");
    } else {
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let fetch = script_dir.join("fetch_uploads.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            fetch.to_string_lossy().as_ref(),
            "--server",
            &args.server.to_string(),
            "--ticket-id",
            &args.ticket_id.to_string(),
            "--landing-dir",
            landing_dir.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");

        let output = cmd.output()?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }

        // The ticket directory should have been created by the fetch
        if !ticket_dir.is_dir() {
            bail!(
                r#"Failed to create ticket directory "{}""#,
                ticket_dir.display()
            );
        }
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

    //let mut num_simulations = ticket_dirs.len();
    //let mut num_trajectories = 0;
    //for ticket_dir in ticket_dirs {
    //    let path = ticket_dir.join("mdrepo-metadata.toml");
    //    let meta = Meta::from_file(&path)?;
    //    num_trajectories += meta.trajectory_file_names.len();
    //}

    //debug!(
    //    "Found {num_trajectories} trajector{} in {num_simulations} simulation{}",
    //    if num_trajectories == 1 { "" } else { "s" },
    //    if num_simulations == 1 { "" } else { "s" },
    //);
    //let num_blast_threads = min(num_cpus::get(), num_trajectories) / num_trajectories;

    // Resolve the DB-feedback context once, up front. If the database is
    // unreachable (or the env DSN is missing) we log and process anyway with
    // feedback disabled -- recording upload status must never block processing.
    // This runs even for a dry run, so the ticket is still fetched and
    // validated; only the writes below are suppressed.
    let ticket_id = args.ticket_id as i64;
    let feedback = build_feedback_ctx(&args.server, ticket_id);

    // Every DB write is gated on `Some`, so withholding the context in a dry
    // run makes the run read-only.
    let writes = if args.dry_run {
        info!("DRY RUN: no upload instances, messages, or ticket updates written");
        None
    } else {
        feedback.as_ref()
    };

    // Process every landing subdirectory in parallel. Each task opens its own
    // DB connection (a Diesel PgConnection can't be shared across threads), so
    // parallelism is unchanged; we only collect per-landing success to decide
    // whether the whole ticket is complete.
    let start = Instant::now();
    let results: Vec<bool> = ticket_dirs
        .into_par_iter()
        .map(|ticket_dir| {
            process_landing(&ticket_dir, args, script_dir, &work_dir, writes)
        })
        .collect();

    // Only flip processing_complete when every landing succeeded.
    let all_ok = !results.is_empty() && results.iter().all(|&ok| ok);
    if let Some(ctx) = writes {
        if all_ok {
            mark_ticket_complete(ctx);
        } else {
            info!(
                "Ticket {ticket_id}: not all directories processed \
                 successfully; leaving processing_complete unchanged"
            );
        }
    }

    info!(
        r#"Done processing ticket "{}" in {:?}"#,
        args.ticket_id,
        start.elapsed()
    );

    Ok(())
}

// --------------------------------------------------
/// DB context shared (read-only) across the parallel per-landing tasks. Each
/// task opens its own `PgConnection` from `db_url` because a Diesel
/// `PgConnection` cannot be shared across threads.
struct FeedbackCtx {
    db_url: String,
    ticket_id: i64,
    user_id: i64,
    orcid: String,
}

// --------------------------------------------------
/// The DSN env var for a given server.
fn dsn_for(server: &Server) -> &'static str {
    match server {
        Server::Production => "PRODUCTION_DSN",
        Server::Staging => "STAGING_DSN",
    }
}

// --------------------------------------------------
/// Read the DSN, connect, and load the ticket's user/orcid. Returns `None`
/// (feedback disabled) on any failure so processing can proceed regardless.
fn build_feedback_ctx(server: &Server, ticket_id: i64) -> Option<FeedbackCtx> {
    let env_key = dsn_for(server);
    let url = match env::var(env_key) {
        Ok(url) => url,
        Err(e) => {
            info!("DB feedback disabled ({env_key}: {e})");
            return None;
        }
    };
    let mut conn = match mdr_db::connect(&url) {
        Ok(conn) => conn,
        Err(e) => {
            info!("DB feedback disabled (connect: {e})");
            return None;
        }
    };
    match ops::get_ticket(&mut conn, ticket_id) {
        Ok(ticket) => Some(FeedbackCtx {
            db_url: url,
            ticket_id,
            user_id: ticket.created_by_id,
            orcid: ticket.orcid.unwrap_or_else(|| "NA".to_string()),
        }),
        Err(e) => {
            info!("DB feedback disabled (get_ticket {ticket_id}: {e})");
            None
        }
    }
}

// --------------------------------------------------
/// Process a single landing subdirectory and, when feedback is enabled, record
/// its upload instance + status messages. Returns whether processing succeeded.
fn process_landing(
    ticket_dir: &Path,
    args: &TicketArgs,
    script_dir: &Path,
    work_dir: &Path,
    feedback: Option<&FeedbackCtx>,
) -> bool {
    let ticket_start = Instant::now();
    debug!(r#"Processing ticket directory "{}""#, ticket_dir.display());

    let landing_id = ticket_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    // Open a per-task connection and ensure the upload instance exists *before*
    // processing, so a failure can still be recorded against it.
    let mut db: Option<(PgConnection, i64)> = None;
    if let Some(ctx) = feedback {
        match mdr_db::connect(&ctx.db_url) {
            Ok(mut conn) => {
                let filenames = collect_filenames(ticket_dir).join(", ");
                match ensure_upload_instance(
                    &mut conn,
                    ctx.ticket_id,
                    &landing_id,
                    ctx.user_id,
                    &ctx.orcid,
                    &filenames,
                ) {
                    Ok(upload_id) => {
                        log_message(
                            &mut conn,
                            upload_id,
                            "Processing started",
                            false,
                            false,
                        );
                        db = Some((conn, upload_id));
                    }
                    Err(e) => info!(
                        "Failed to record upload instance for landing \
                         '{landing_id}': {e}"
                    ),
                }
            }
            Err(e) => {
                info!("Failed to connect for landing '{landing_id}': {e}")
            }
        }
    }

    let outcome = process::process(&ProcessArgs {
        input_dir: ticket_dir.to_path_buf(),
        script_dir: Some(script_dir.to_path_buf()),
        work_dir: Some(work_dir.to_path_buf()),
        out_dir: None,
        server: args.server.clone(),
        reprocess_simulation_id: None,
        // The TOML will have already been validated, so allow missing IDs
        no_id: true,
        force: args.force,
        dry_run: args.dry_run,
        replace_original_files: false,
    });

    let success = match &outcome {
        Ok(result) => {
            if let Some((conn, upload_id)) = db.as_mut() {
                if result.errors.is_empty() {
                    log_message(
                        conn,
                        *upload_id,
                        "Processing completed successfully",
                        false,
                        false,
                    );
                } else {
                    for warning in &result.errors {
                        log_message(conn, *upload_id, warning, false, true);
                    }
                }
                update_instance_result(
                    conn,
                    *upload_id,
                    true,
                    result.simulation_id,
                );
            }
            if !result.errors.is_empty() {
                info!(
                    "Errors for {}:\n{}",
                    ticket_dir.display(),
                    result.errors.join("\n")
                );
            }
            true
        }
        Err(e) => {
            if let Some((conn, upload_id)) = db.as_mut() {
                log_message(
                    conn,
                    *upload_id,
                    &format!("Processing failed: {e}"),
                    true,
                    false,
                );
                update_instance_result(conn, *upload_id, false, None);
            }
            debug!(
                r#"Error processing ticket directory "{}": {e}"#,
                ticket_dir.display()
            );
            false
        }
    };

    debug!(
        r#"Finished ticket directory "{}" in {:?}"#,
        ticket_dir.display(),
        ticket_start.elapsed()
    );
    success
}

// --------------------------------------------------
/// Upsert the upload instance for `(ticket_id, landing_id)`; returns its id.
fn ensure_upload_instance(
    conn: &mut PgConnection,
    ticket_id: i64,
    landing_id: &str,
    user_id: i64,
    orcid: &str,
    filenames: &str,
) -> Result<i64> {
    let (_, existing) =
        ops::list_upload_instances(conn, None, None, Some(ticket_id), true, None, None)?;

    if let Some(instance) = existing
        .into_iter()
        .find(|inst| inst.landing_id.as_deref() == Some(landing_id))
    {
        return Ok(instance.id);
    }

    let instance = ops::insert_upload_instance(
        conn,
        NewUploadInstance {
            created_on: Utc::now(),
            simulation_id: None,
            user_id: Some(user_id),
            successful: None,
            lead_contributor_orcid: orcid.to_string(),
            filenames: Some(filenames.to_string()),
            ticket_id: Some(ticket_id),
            landing_id: Some(landing_id.to_string()),
        },
    )?;
    Ok(instance.id)
}

// --------------------------------------------------
/// Insert a status message; failures are logged, not propagated.
fn log_message(
    conn: &mut PgConnection,
    upload_id: i64,
    message: &str,
    is_error: bool,
    is_warning: bool,
) {
    if let Err(e) = ops::insert_upload_message(
        conn,
        NewUploadMessage {
            timestamp: Utc::now(),
            message: message.to_string(),
            simulation_upload_id: upload_id,
            is_error,
            is_warning,
        },
    ) {
        info!("Failed to record upload message: {e}");
    }
}

// --------------------------------------------------
/// Update the instance's `successful` flag and (best-effort) `simulation_id`.
fn update_instance_result(
    conn: &mut PgConnection,
    upload_id: i64,
    successful: bool,
    simulation_id: Option<u32>,
) {
    if let Err(e) = ops::update_upload_instance(
        conn,
        upload_id,
        UploadInstanceUpdate {
            successful: Some(Some(successful)),
            simulation_id: simulation_id.map(|id| Some(id as i64)),
            ..Default::default()
        },
    ) {
        info!("Failed to update upload instance {upload_id}: {e}");
    }
}

// --------------------------------------------------
/// Set `md_ticket.processing_complete = true`.
fn mark_ticket_complete(ctx: &FeedbackCtx) {
    let mut conn = match mdr_db::connect(&ctx.db_url) {
        Ok(conn) => conn,
        Err(e) => {
            info!("Failed to connect to mark ticket complete: {e}");
            return;
        }
    };
    match ops::update_ticket(
        &mut conn,
        ctx.ticket_id,
        TicketUpdate {
            processing_complete: Some(true),
            ..Default::default()
        },
    ) {
        Ok(_) => info!("Marked ticket {} processing_complete", ctx.ticket_id),
        Err(e) => info!(
            "Failed to set processing_complete for ticket {}: {e}",
            ctx.ticket_id
        ),
    }
}

// --------------------------------------------------
/// Collect uploaded filenames in a landing dir, excluding submission metadata.
fn collect_filenames(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| entry.file_name().to_str().map(String::from))
            .filter(|name| !name.starts_with("mdrepo-submission."))
            .collect(),
        Err(_) => vec![],
    };
    names.sort();
    names
}
