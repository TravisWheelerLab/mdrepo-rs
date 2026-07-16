use crate::{
    process,
    types::{ProcessArgs, Server, SubmissionCompleteJson, TicketArgs, TicketInfo},
};
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use diesel::pg::PgConnection;
use dotenvy::dotenv;
//use libmdrepo::metadata::Meta;
use libmdrepo::{
    common::{get_md5, read_file},
    constants::MAX_FILE_SIZE_BYTES,
};
use log::{debug, info};
use mdr_db::{
    models::{
        NewUploadInstance, NewUploadMessage, TicketUpdate, UploadInstanceUpdate,
    },
    ops,
};
use rayon::prelude::*;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use which::which;

/// Manifest the uploader writes into a landing directory once every file has
/// been transferred, recording the size and MD5 of each.
pub const COMPLETED_JSON: &str = "mdrepo-submission.completed.json";

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
    let ok_count = results.iter().filter(|&&ok| ok).count();
    let all_ok = !results.is_empty() && ok_count == results.len();
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

    // Fail the whole run if any landing failed, so callers (and the exit code)
    // see the failure rather than a false success.
    if !all_ok {
        bail!(
            "Ticket {ticket_id}: {} of {} director{} failed to process",
            results.len() - ok_count,
            results.len(),
            if results.len() == 1 { "y" } else { "ies" }
        );
    }

    Ok(())
}

// --------------------------------------------------
/// Check a landing directory's files against the manifest the uploader wrote.
/// This is purely a transfer-integrity check -- it never parses the metadata
/// TOML -- so an error here means the download is incomplete or corrupt, not
/// that the submission itself is bad. Callers that cannot assume a manifest
/// (any directory not fetched from a ticket) should test for `COMPLETED_JSON`
/// before calling.
pub fn check_manifest(dir: &Path) -> Result<Vec<String>> {
    let completed_path = dir.join(COMPLETED_JSON);
    if !completed_path.is_file() {
        bail!(r#"Missing "{}""#, completed_path.display());
    }

    let completed: SubmissionCompleteJson =
        serde_json::from_str(&read_file(&completed_path)?).map_err(|e| {
            anyhow!(r#"Failed to parse "{}": {e}"#, completed_path.display())
        })?;

    if completed.total_filenum as usize != completed.files.len() {
        bail!(
            "Expected {} file(s) but completed JSON has {}",
            completed.total_filenum,
            completed.files.len()
        );
    }

    // Ensure that some files were uploaded
    if completed.files.is_empty() {
        bail!(r#""{}" contains no "files"#, completed_path.display());
    }

    let mut errors = vec![];

    // Keyed by MD5 so that two uploaded files with identical content can be
    // reported together; ordered so the message is stable across runs.
    let mut md5_hashes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut total_file_size = 0;

    // Check file hashes/sizes
    for file in &completed.files {
        total_file_size += file.size;

        let path = dir.join(&file.irods_path);
        if !path.exists() {
            errors.push(format!(r#"Missing expected file "{}""#, file.irods_path));
            continue;
        }

        let size = path.metadata()?.len();
        let local_md5 = get_md5(&path)?;

        debug!(
            r#"Checking "{}" = expected md5 {}, size {}"#,
            file.irods_path,
            if file.md5_hash == local_md5 {
                "OK"
            } else {
                "BAD"
            },
            if file.size == size { "OK" } else { "Bad" }
        );

        if file.md5_hash != local_md5 {
            errors.push(format!(
                r#""{}" MD5 "{}" does not match manifest "{}""#,
                file.irods_path, local_md5, file.md5_hash
            ));
        }

        if file.size != size {
            errors.push(format!(
                r#""{}" size "{}" does not match manifest "{}""#,
                file.irods_path, size, file.size
            ))
        }

        md5_hashes
            .entry(local_md5)
            .or_default()
            .push(file.irods_path.clone());
    }

    for (hash, files) in md5_hashes {
        if files.len() > 1 {
            errors.push(format!(
                r#"File hash "{hash}" is duplicated: [{}]"#,
                files.join(", ")
            ))
        }
    }

    // The manifest should agree with itself.
    if completed.total_filesize != total_file_size {
        errors.push(format!(
            r#""{}" total_filesize "{}" does not match the sum of its file sizes "{}""#,
            COMPLETED_JSON, completed.total_filesize, total_file_size
        ));
    }

    // Check max upload size
    if total_file_size > MAX_FILE_SIZE_BYTES {
        errors.push(format!(
            "Total file size ({total_file_size}) exceeds limit {MAX_FILE_SIZE_BYTES}"
        ));
    }

    Ok(errors)
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

    // Verify the download against its manifest before processing. This must
    // come first: processing rewrites the metadata TOML in place, so anything
    // downstream is checking bytes we produced rather than bytes the submitter
    // uploaded. Failing here also keeps the two failure classes apart -- these
    // are transfer errors, whereas a later metadata error is the submission's.
    let outcome = match check_manifest(ticket_dir) {
        Ok(errors) if !errors.is_empty() => Err(anyhow!(
            "Upload is incomplete or corrupt:\n{}",
            errors.join("\n")
        )),
        Err(e) => Err(e),
        Ok(_) => process::process(&ProcessArgs {
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
        }),
    };

    let success = match &outcome {
        Ok(result) => {
            // process::process returns Ok only on success (fatal errors bail),
            // so this landing succeeded; warnings are non-fatal and are noted.
            if let Some((conn, upload_id)) = db.as_mut() {
                for warning in &result.warnings {
                    log_message(conn, *upload_id, warning, false, true);
                }
                log_message(
                    conn,
                    *upload_id,
                    "Processing completed successfully",
                    false,
                    false,
                );
                update_instance_result(
                    conn,
                    *upload_id,
                    true,
                    result.simulation_id,
                );
            }
            if !result.warnings.is_empty() {
                info!(
                    "Warnings for {}:\n{}",
                    ticket_dir.display(),
                    result.warnings.join("\n")
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

#[cfg(test)]
mod tests {
    use super::*;
    use libmdrepo::common::get_md5;
    use tempfile::tempdir;

    /// A landing dir holding one uploaded file, plus a manifest describing it.
    fn write_upload(dir: &Path) {
        fs::write(dir.join("sim.xtc"), b"trajectory").unwrap();
        let md5 = get_md5(&dir.join("sim.xtc")).unwrap();
        let json = format!(
            r#"{{"total_filenum":1,"total_filesize":10,"status":"completed","files":[{{"irods_path":"sim.xtc","size":10,"md5_hash":"{md5}"}}]}}"#
        );
        fs::write(dir.join(COMPLETED_JSON), json).unwrap();
    }

    #[test]
    fn accepts_intact_upload() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        assert!(check_manifest(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn missing_completed_json_errs() {
        let dir = tempdir().unwrap();
        let err = check_manifest(dir.path()).unwrap_err();
        assert!(err.to_string().contains("Missing"));
    }

    #[test]
    fn wrong_filenum_errs() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        let json = r#"{"total_filenum":5,"total_filesize":0,"status":"completed","files":[{"irods_path":"sim.xtc","size":9,"md5_hash":"abc"}]}"#;
        fs::write(dir.path().join(COMPLETED_JSON), json).unwrap();
        assert!(check_manifest(dir.path()).is_err());
    }

    #[test]
    fn missing_file_returns_error() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        let json = r#"{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{"irods_path":"ghost.xtc","size":0,"md5_hash":"abc123abc123abc123abc123abc12300"}]}"#;
        fs::write(dir.path().join(COMPLETED_JSON), json).unwrap();
        let errors = check_manifest(dir.path()).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing") && e.contains("ghost.xtc")));
    }

    #[test]
    fn md5_mismatch_returns_error() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        let json = r#"{"total_filenum":1,"total_filesize":10,"status":"completed","files":[{"irods_path":"sim.xtc","size":10,"md5_hash":"00000000000000000000000000000000"}]}"#;
        fs::write(dir.path().join(COMPLETED_JSON), json).unwrap();
        let errors = check_manifest(dir.path()).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("MD5") && e.contains("sim.xtc")));
    }

    #[test]
    fn size_mismatch_returns_error() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        let md5 = get_md5(&dir.path().join("sim.xtc")).unwrap();
        let json = format!(
            r#"{{"total_filenum":1,"total_filesize":9999,"status":"completed","files":[{{"irods_path":"sim.xtc","size":9999,"md5_hash":"{md5}"}}]}}"#
        );
        fs::write(dir.path().join(COMPLETED_JSON), json).unwrap();
        let errors = check_manifest(dir.path()).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("size") && e.contains("sim.xtc")));
    }

    #[test]
    fn duplicate_md5_returns_error() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("copy_a.dat"), b"identical").unwrap();
        fs::write(dir.path().join("copy_b.dat"), b"identical").unwrap();
        let md5 = get_md5(&dir.path().join("copy_a.dat")).unwrap();
        let json = format!(
            r#"{{"total_filenum":2,"total_filesize":18,"status":"completed","files":[
                {{"irods_path":"copy_a.dat","size":9,"md5_hash":"{md5}"}},
                {{"irods_path":"copy_b.dat","size":9,"md5_hash":"{md5}"}}
            ]}}"#
        );
        fs::write(dir.path().join(COMPLETED_JSON), json).unwrap();
        let errors = check_manifest(dir.path()).unwrap();
        assert!(errors.iter().any(|e| e.contains("duplicated")));
    }

    #[test]
    fn inconsistent_total_filesize_returns_error() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        let md5 = get_md5(&dir.path().join("sim.xtc")).unwrap();
        let json = format!(
            r#"{{"total_filenum":1,"total_filesize":999,"status":"completed","files":[{{"irods_path":"sim.xtc","size":10,"md5_hash":"{md5}"}}]}}"#
        );
        fs::write(dir.path().join(COMPLETED_JSON), json).unwrap();
        let errors = check_manifest(dir.path()).unwrap();
        assert!(errors.iter().any(|e| e.contains("total_filesize")));
    }

    /// A manifest check must never depend on the metadata TOML, so that a bad
    /// TOML and a bad download stay distinguishable.
    #[test]
    fn ignores_unparsable_metadata_toml() {
        let dir = tempdir().unwrap();
        write_upload(dir.path());
        fs::write(dir.path().join("mdrepo-metadata.toml"), "not {{ valid").unwrap();
        assert!(check_manifest(dir.path()).unwrap().is_empty());
    }
}
