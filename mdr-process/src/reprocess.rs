use crate::{
    process,
    types::{ProcessArgs, ReprocessArgs},
};
use anyhow::{bail, Result};
use dotenvy::dotenv;
use libmdrepo::{common::file_exists, metadata::Meta};
use log::debug;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

// --------------------------------------------------
pub fn reprocess(args: &ReprocessArgs) -> Result<()> {
    dotenv().ok();
    let simulation_id = args.simulation_id;
    let mdrepo_id = format!("MDR{simulation_id:08}");
    debug!("Reprocessing simulation ID {mdrepo_id}");

    let server = &args.server;
    let data_dir = &args.work_dir.join(server.to_string()).join(&mdrepo_id);
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
        dirname: data_dir.clone(),
        script_dir: None,
        out_dir: None,
        json_dir: None,
        server: args.server.clone(),
        simulation_id: Some(simulation_id),
        force: args.force,
        // If it had no PDB/Uniprots before, let it stand
        no_id: true,
    })?;

    if !args.preserve {
        fs::remove_dir_all(data_dir)?;
    }

    Ok(())
}

// --------------------------------------------------
fn irods_fetch(irods_path: &Path, local_path: &PathBuf) -> Result<()> {
    debug!(
        r#"Get "{}" -> "{}""#,
        irods_path.display(),
        local_path.display()
    );

    if file_exists(local_path) {
        debug!("Already downloaded");
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
