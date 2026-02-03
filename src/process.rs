use crate::{metadata::Meta, types::ProcessArgs};
use anyhow::{anyhow, bail, Result};
use log::info;
use std::{fs, path::PathBuf, process::Command};
use which::which;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<()> {
    dbg!(&args);
    let in_dir = &args.dirname;
    let out_dir = args
        .outdir
        .clone()
        .map_or(in_dir.join("processed"), |dir| PathBuf::from(&dir));
    let script_dir = &args.script_dir.clone().unwrap();
    let cpp_traj = &script_dir.join("cpptraj_gmx_traj_manipulation.py");
    if !cpp_traj.is_file() {
        bail!(r#"Missing "{}""#, cpp_traj.display());
    }
    dbg!(&out_dir);

    let meta_path = in_dir.join("mdrepo-metadata.toml");
    let meta = Meta::from_file(&meta_path)?;
    let reqd_file = meta.required_files.unwrap();
    let expected_out_files = &[
        "full.gro",
        "full.pdb",
        "full.xtc",
        "minimal.gro",
        "minimal.pdb",
        "minimal.xtc",
    ]
    .map(|f| out_dir.join(f));
    dbg!(&expected_out_files);

    if expected_out_files.iter().all(exists) {
        info!("Full/minimal files all exist");
    } else {
        let micromamba = which("micromamba")
            .map_err(|e| anyhow!("Failed to find micromamba ({e})"))?;
        info!("Making full/minimal files");
        let cmd = Command::new(micromamba)
            .args([
                "run",
                "-n",
                "simproc",
                &cpp_traj.to_string_lossy().to_string(),
                "-f",
                &in_dir
                    .join(&reqd_file.trajectory_file_name)
                    .to_string_lossy()
                    .to_string(),
                "-c",
                &in_dir
                    .join(&reqd_file.structure_file_name)
                    .to_string_lossy()
                    .to_string(),
                "-t",
                &in_dir
                    .join(&reqd_file.topology_file_name)
                    .to_string_lossy()
                    .to_string(),
                "-o",
                &out_dir.to_string_lossy().to_string(),
            ])
            .output()?;
        dbg!(&cmd);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        let missing: Vec<_> = expected_out_files
            .iter()
            .filter(|f| !exists(f))
            .map(|f| f.to_string_lossy().to_string())
            .collect();

        if !missing.is_empty() {
            bail!("Failed to create: {}", missing.join(", "));
        }
    }

    //Ok(FullMinFiles {
    //    full_gro: out_dir.join("full.gro"),
    //    full_pdb: out_dir.join("full.pdb"),
    //    full_xtc: out_dir.join("full.xtc"),
    //    min_gro: out_dir.join("minimal.gro"),
    //    min_pdb: out_dir.join("minimal.pdb"),
    //    min_xtc: out_dir.join("minimal.xtc"),
    //})

    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    let min_xtc = out_dir.join("minimal.xtc");
    let min_pdb = out_dir.join("minimal.pdb");
    let sampled_trajectory = out_dir.join("sampled.xtc");
    if exists(&sampled_trajectory) {
        info!("Sampled trajectory exists");
    } else {
        info!("Creating sampled trajectory");
        let sampler = script_dir.join("sample_trajectory.py");
        let cmd = Command::new(&uv)
            .current_dir(&script_dir)
            .args([
                "run",
                &sampler.to_string_lossy().to_string(),
                "--trajectory",
                &min_xtc.to_string_lossy().to_string(),
                "--structure",
                &min_pdb.to_string_lossy().to_string(),
                "--outfile",
                &sampled_trajectory.to_string_lossy().to_string(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?.to_string());

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !exists(&sampled_trajectory) {
            bail!(r#"Failed to create "{}""#, sampled_trajectory.display());
        }
    }

    let thumbnail = out_dir.join("thumbnail.png");
    if exists(&thumbnail) {
        info!("Thumbnail exists");
    } else {
        info!("Creating thumbnail");
        let preview = script_dir.join("create_preview.py");
        let cmd = Command::new(&uv)
            .current_dir(&script_dir)
            .args([
                "run",
                &preview.to_string_lossy().to_string(),
                "--trajectory",
                &sampled_trajectory.to_string_lossy().to_string(),
                "--structure",
                &min_pdb.to_string_lossy().to_string(),
                "--out-file",
                &thumbnail.to_string_lossy().to_string(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?.to_string());

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !exists(&thumbnail) {
            bail!(r#"Failed to create "{}""#, thumbnail.display());
        }
    }

    Ok(())
}

// --------------------------------------------------
fn exists(file: &PathBuf) -> bool {
    if let Ok(meta) = fs::metadata(file) {
        meta.is_file() && meta.len() > 0
    } else {
        false
    }
}
