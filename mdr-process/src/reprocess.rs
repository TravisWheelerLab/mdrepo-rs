use crate::{
    process,
    types::{ProcessArgs, ReprocessArgs},
};
use anyhow::{anyhow, bail, Result};
use dotenvy::dotenv;
use libmdrepo::{common::file_exists, metadata::Meta};
use log::{debug, info};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

// --------------------------------------------------
pub fn reprocess(args: &ReprocessArgs) -> Result<()> {
    dotenv().ok();
    let work_dir = args.work_dir.clone().unwrap_or(PathBuf::from(
        env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
    ));
    let simulation_id = args.simulation_id;
    let mdrepo_id = format!("MDR{simulation_id:08}");
    debug!("Reprocessing simulation ID {mdrepo_id}");

    let server = args.server.clone();
    let data_dir = &work_dir
        .join("reprocess")
        .join(server.to_string())
        .join(&mdrepo_id);

    if !data_dir.is_dir() {
        fs::create_dir_all(data_dir)?;
    }

    let irods_dir =
        format!("/iplant/home/shared/mdrepo/{server}/release/{mdrepo_id}/original");
    let irods_dir = Path::new(&irods_dir);
    debug!(r#"irods_dir = "{}""#, irods_dir.display());

    let meta_filename = "mdrepo-metadata.toml";
    let meta_local_path = data_dir.join(meta_filename);
    irods_fetch(&irods_dir.join(meta_filename), &meta_local_path)?;

    let meta = Meta::from_file(&meta_local_path)?;
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

    process::process(&ProcessArgs {
        input_dir: data_dir.clone(),
        script_dir: None,
        work_dir: Some(work_dir),
        out_dir: None,
        server,
        reprocess_simulation_id: Some(simulation_id),
        force: args.force,
        no_id: true, // If it had no PDB/Uniprots before, let it stand
        dry_run: args.dry_run,
    })?;

    if !args.preserve {
        info!(r#"Removing "{}""#, data_dir.display());
        fs::remove_dir_all(data_dir)?;
    }

    Ok(())
}

// --------------------------------------------------
fn irods_fetch(irods_path: &Path, local_path: &Path) -> Result<()> {
    if file_exists(local_path) {
        debug!(
            r#"Already downloaded "{}""#,
            irods_path.file_name().expect("filename").display()
        );
    } else {
        let mut cmd = Command::new("gocmd");
        cmd.args([
            "get",
            irods_path.to_string_lossy().as_ref(),
            local_path.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
        }
    }
    Ok(())
}
