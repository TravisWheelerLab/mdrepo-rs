use crate::{common::file_exists, metadata::Meta, process, types::ReprocessArgs};
use anyhow::{bail, Result};
use log::debug;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

// --------------------------------------------------
pub fn reprocess(args: &ReprocessArgs) -> Result<()> {
    let simulation_id = args.simulation_id;
    let mdrepo_id = format!("MDR{simulation_id:08}");
    debug!("Reprocessing simulation ID {mdrepo_id}");
    let data_dir = &args.work_dir.join(&mdrepo_id);
    if !data_dir.is_dir() {
        fs::create_dir_all(&data_dir)?;
    }

    let server = &args.server;
    let irods_dir =
        &format!("/iplant/home/shared/mdrepo/{server}/release/{mdrepo_id}/original");
    let irods_dir = Path::new(&irods_dir);
    let meta_filename = "mdrepo-metadata.toml";
    let meta_irods = &irods_dir.join(&meta_filename);
    let meta_path = &data_dir.join(&meta_filename);
    irods_fetch(&meta_irods, &meta_path)?;
    let meta = Meta::from_file(&meta_path)?;

    //let trajectory_filename = meta.required_files.trajectory_file_name;
    //let trajectory_path = &data_dir.join(&trajectory_filename);
    //irods_fetch(&irods_dir.join(&trajectory_filename), &trajectory_path)?;

    //let structure_filename = meta.required_files.structure_file_name;
    //let structure_path = &data_dir.join(&structure_filename);
    //irods_fetch(&irods_dir.join(&structure_filename), &structure_path)?;

    //let topology_filename = meta.required_files.topology_file_name;
    //let topology_path = &data_dir.join(&topology_filename);
    //irods_fetch(&irods_dir.join(&topology_filename), &topology_path)?;

    for filename in &[
        &meta.trajectory_file_name,
        &meta.structure_file_name,
        &meta.topology_file_name,
    ] {
        irods_fetch(&irods_dir.join(&filename), &data_dir.join(&filename))?;
    }

    if let Some(addl_files) = meta.additional_files {
        for file in addl_files {
            irods_fetch(
                &irods_dir.join(&file.file_name),
                &data_dir.join(&file.file_name),
            )?;
        }
    }

    let processed_dir = &data_dir.join("processed");
    let script_dir = &args.script_dir.clone().unwrap();
    let processed_files = process::make_processed_files(
        &meta_path,
        &data_dir,
        &processed_dir,
        &script_dir,
    )?;
    dbg!(&processed_files);

    //let import_json = data_dir.join(format!("{mdrepo_id}.json"));
    process::make_import_json(
        &meta_path,
        &data_dir,
        &script_dir,
        &processed_files,
        //&import_json,
        Some(simulation_id),
    )?;

    Ok(())
}

// --------------------------------------------------
fn irods_fetch(irods_path: &PathBuf, local_path: &PathBuf) -> Result<()> {
    if !file_exists(&local_path) {
        debug!(
            r#"Get "{}" -> "{}""#,
            irods_path.display(),
            local_path.display()
        );
        let cmd = Command::new("gocmd")
            .args([
                "get",
                &irods_path.to_string_lossy().to_string(),
                &local_path.to_string_lossy().to_string(),
            ])
            .output()?;
        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }
    }
    Ok(())
}
