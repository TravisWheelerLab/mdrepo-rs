use crate::{
    process,
    types::{ProcessArgs, ReprocessArgs},
};
use anyhow::{Result, anyhow, bail};
use dotenvy::dotenv;
use libmdrepo::{
    common::{file_exists, get_simulation_id},
    metadata::Meta,
};
use log::debug;
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
    let simulation_id = get_simulation_id(&args.simulation_id)?;
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
    for filename in &[&meta.structure_file_name, &meta.topology_file_name] {
        irods_fetch(&irods_dir.join(filename), &data_dir.join(filename))?;
    }

    // When a simulation has multiple trajectories, `process` does not upload them
    // individually: it bundles every trajectory file (including the representative
    // one) into "trajectories.tar". Fetching each `trajectory_file_names` entry by
    // name would therefore 404 for all but, at best, a lone representative. Instead
    // fetch the tarball and extract it to recover every individual trajectory file
    // the metadata references. Single-trajectory simulations have no tarball, so
    // fetch that one file directly.
    if meta.trajectory_file_names.len() > 1 {
        let tar_name = "trajectories.tar";
        let tar_local = data_dir.join(tar_name);
        irods_fetch(&irods_dir.join(tar_name), &tar_local)?;
        let mut cmd = Command::new("tar");
        cmd.current_dir(data_dir).args(["-xf", tar_name]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }
    } else {
        for filename in &meta.trajectory_file_names {
            irods_fetch(&irods_dir.join(filename), &data_dir.join(filename))?;
        }
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
        agent_imported: false,
        dry_run: args.dry_run,
        replace_original_files: false,
        blast_num_threads: args.blast_num_threads,
        transfer_threads: args.transfer_threads,
        ticket_id: None,
    })?;

    if !args.dry_run && !args.preserve {
        debug!(r#"Removing "{}""#, data_dir.display());
        fs::remove_dir_all(data_dir)?;
    }

    Ok(())
}

// --------------------------------------------------
fn irods_fetch(irods_path: &Path, local_path: &Path) -> Result<()> {
    if file_exists(local_path) {
        debug!(
            r#"Already downloaded "{}""#,
            irods_path
                .file_name()
                .ok_or_else(|| anyhow!("No filename for '{}'", irods_path.display()))?
                .to_string_lossy()
        );
    } else {
        let mut cmd = Command::new("gocmd");
        cmd.args([
            "get",
            // One transfer thread, not gocmd's default of 5 per file.
            //
            // CyVerse's 500-connection ceiling is global across all of their
            // users and we have been holding 200-400 of it, so a single file
            // claiming five connections is expensive well beyond this process.
            // It costs almost nothing: measured 2026-08-20 on an 82 MB release
            // object, the default took 16.9s and 4 connections against 18.5s
            // and 2 for one thread per file.
            //
            // This fetches one file per invocation, so --thread_num_per_file
            // and --single_threaded would behave the same here; the explicit
            // per-file cap is the one that stays correct if this ever passes
            // several sources at once.
            "--thread_num_per_file",
            "1",
            irods_path.to_string_lossy().as_ref(),
            local_path.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }
    }
    Ok(())
}
