use crate::{
    process,
    types::{ProcessArgs, ReprocessArgs},
};
use anyhow::{bail, Result};
use log::{debug, info};
use std::{fs, process::Command};

// --------------------------------------------------
pub fn reprocess(args: &ReprocessArgs) -> Result<()> {
    let _ = dotenv::dotenv();
    let simulation_id = args.simulation_id;
    let mdrepo_id = format!("MDR{simulation_id:08}");
    debug!("Reprocessing simulation ID {mdrepo_id}");

    let data_dir = &args.work_dir.join(&mdrepo_id);
    if !args.data_dir.is_dir() {
        fs::create_dir_all(&args.work_dir)?;
    }

    let server = &args.server;
    let irods_dir =
        &format!("/iplant/home/shared/mdrepo/{server}/release/{mdrepo_id}/original");

    for filename in &[
        &meta.trajectory_file_name,
        &meta.structure_file_name,
        &meta.topology_file_name,
    ] {
        irods_fetch(&irods_dir.join(filename), &data_dir.join(filename))?;
    }

    if let Some(addl_files) = meta.additional_files {
        for file in addl_files {
            irods_fetch(
                &irods_dir.join(&file.file_name),
                &data_dir.join(&file.file_name),
            )?;
        }
    }

    match process::process(&ProcessArgs {
        dirname: data_dir.clone(),
        script_dir: None,
        out_dir: None,
        json_dir: None,
        server: args.server.clone(),
        simulation_id: Some(simulation_id),
    }) {
        Ok(()) => {
            info!("Success");
        }
        Err(e) => info!("Error: {e}"),
    }

    Ok(())
}

// --------------------------------------------------
fn irods_fetch(irods_path: &Path, local_path: &PathBuf) -> Result<()> {
    if !file_exists(local_path) {
        debug!(
            r#"Get "{}" -> "{}""#,
            irods_path.display(),
            local_path.display()
        );
        let cmd = Command::new("gocmd")
            .args([
                "get",
                irods_path.to_string_lossy().as_ref(),
                local_path.to_string_lossy().as_ref(),
            ])
            .output()?;
        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }
    }
    Ok(())
}
