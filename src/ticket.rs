use crate::{
    common::read_file,
    process,
    types::{Import, ImportResult, ProcessArgs, PushResult, TicketArgs},
};
use anyhow::{anyhow, bail, Result};
use log::{debug, info};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
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

    let mut imports: Vec<Import> = vec![];
    for entry in fs::read_dir(&ticket_dir)? {
        let entry = entry?;
        let entry = entry.path();
        if entry.is_dir() {
            debug!(r#"Process "{}""#, entry.display());
            match process::process(&ProcessArgs {
                dirname: entry.clone(),
                script_dir: Some(script_dir.clone()),
                out_dir: None,
                json_dir: None,
                server: args.server.clone(),
            }) {
                Ok(import_json) => {
                    info!("Success");
                    imports.push(Import {
                        dirname: entry.clone(),
                        import_json: import_json.clone(),
                    });
                }
                Err(e) => info!("Error: {e}"),
            }
        }
    }

    // TODO: Move "for" loop code into functions of process
    debug!("imports = {imports:?}");
    let mut import_results: Vec<ImportResult> = vec![];
    let import_script = script_dir.join("import_preprocessed.py");
    for import in imports {
        info!(r#"Import "{}""#, import.import_json.display());
        let out_file = &import.dirname.join("processed").join("imported.json");
        let cmd = Command::new(&uv)
            .current_dir(&script_dir)
            .args([
                "run",
                &import_script.to_string_lossy().to_string(),
                "--file",
                &import.import_json.to_string_lossy().to_string(),
                "--data-dir",
                &import.dirname.to_string_lossy().to_string(),
                "--server",
                &args.server.to_string(),
                "--out-file",
                &out_file.to_string_lossy().to_string(),
            ])
            .output()?;

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        // The ticket directory should have been created by the fetch
        if !out_file.is_file() {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }

        let import_res: ImportResult = serde_json::from_str(&read_file(&out_file)?)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display()))?;

        import_results.push(import_res);
    }

    let push_script = script_dir.join("push_sim_files.py");
    for res in import_results {
        info!(
            r#"Push files for "{}" -> simuation "{}""#,
            res.filename, res.simulation_id
        );

        let data_dir = Path::new(&res.data_dir);
        let out_file = &data_dir.join("processed").join("pushed.json");
        let cmd = Command::new(&uv)
            .current_dir(&script_dir)
            .args([
                "run",
                &push_script.to_string_lossy().to_string(),
                "--file",
                &res.filename,
                "--simulation-id",
                &res.simulation_id.to_string(),
                "--server",
                &args.server.to_string(),
                "--data-dir",
                &res.data_dir,
                "--out-file",
                &out_file.to_string_lossy().to_string(),
            ])
            .output()?;

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !out_file.is_file() {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }

        let push_res: Vec<PushResult> = serde_json::from_str(&read_file(&out_file)?)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display()))?;
        debug!("{push_res:?}");
    }

    info!(r#"Done processing ticket "{}""#, args.ticket_id);

    Ok(())
}
