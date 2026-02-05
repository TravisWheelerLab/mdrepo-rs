use crate::{
    metadata::{Meta, MoleculeType},
    types::{
        Duration, FullMinFiles, PdbEntry, PdbResponse, ProcessArgs, ProteinSequence,
        RmsdRmsf, UniprotEntry, UniprotResponse,
    },
};
use anyhow::{anyhow, bail, Result};
use log::info;
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::Command,
};
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
    dbg!(&out_dir);

    let files = make_full_min_files(&in_dir, &out_dir, &script_dir)?;

    let sampled_trajectory = out_dir.join("sampled.xtc");
    sample_trajectory(
        &files.min_xtc,
        &files.min_pdb,
        &sampled_trajectory,
        &script_dir,
    )?;

    let thumbnail = out_dir.join("thumbnail.png");
    make_thumbnail(&thumbnail, &sampled_trajectory, &files.min_pdb, &script_dir)?;

    let sequence_file = out_dir.join("sequence.json");
    let sequence = get_sequence(&files.full_pdb, &sequence_file, &script_dir)?;
    dbg!(&sequence);

    //let rmsd_rmsf_file = out_dir.join("rmsd_rmsf.json");
    //let rmsd_rmsf =
    //    get_rmsd_rmsf(&files.min_pdb, &files.min_xtc, &rmsd_rmsf_file, &script_dir)?;
    //dbg!(&rmsd_rmsf);

    let duration_file = out_dir.join("duration.json");
    let duration = get_duration(&files.full_xtc, &duration_file)?;
    dbg!(&duration);

    let meta = Meta::from_file(&files.meta_toml)?;
    let topology = in_dir.join(meta.required_files.topology_file_name);
    let topology_hash = get_topology_hash(&topology)?;
    dbg!(&topology_hash);

    dbg!(&meta.proteins);
    for protein in &meta.proteins {
        match (
            protein.molecule_id_type.clone(),
            protein.molecule_id.clone(),
        ) {
            (Some(MoleculeType::Uniprot), Some(uniprot_id)) => {
                let uniprot = get_uniprot_entry(&uniprot_id)?;
                dbg!(&uniprot);
            }
            (Some(MoleculeType::PDB), Some(pdb_id)) => {
                let pdb = get_pdb_entry(&pdb_id)?;
                dbg!(&pdb);
            }
            _ => println!("Handle {protein:?}"),
        }
    }

    //let json_dir = &args.json_dir.clone().unwrap();
    //let in_dir_basename = &in_dir.file_name().unwrap().to_string_lossy().to_string();
    //let import_json = json_dir.join(format!("{in_dir_basename}.json"));
    //make_import_json(&files, &sampled_trajectory, &thumbnail, &import_json)?;

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

// --------------------------------------------------
fn make_thumbnail(
    thumbnail: &PathBuf,
    sampled_trajectory: &PathBuf,
    min_pdb: &PathBuf,
    script_dir: &PathBuf,
) -> Result<()> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
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
fn make_full_min_files(
    in_dir: &PathBuf,
    out_dir: &PathBuf,
    script_dir: &PathBuf,
) -> Result<FullMinFiles> {
    let meta_path = in_dir.join("mdrepo-metadata.toml");
    let meta = Meta::from_file(&meta_path)?;
    let reqd_file = meta.required_files;
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
        let cpp_traj = &script_dir.join("cpptraj_gmx_traj_manipulation.py");
        if !cpp_traj.is_file() {
            bail!(r#"Missing "{}""#, cpp_traj.display());
        }
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

    Ok(FullMinFiles {
        meta_toml: meta_path,
        full_gro: out_dir.join("full.gro"),
        full_pdb: out_dir.join("full.pdb"),
        full_xtc: out_dir.join("full.xtc"),
        min_gro: out_dir.join("minimal.gro"),
        min_pdb: out_dir.join("minimal.pdb"),
        min_xtc: out_dir.join("minimal.xtc"),
    })
}

// --------------------------------------------------
fn get_rmsd_rmsf(
    min_pdb: &PathBuf,
    min_xtc: &PathBuf,
    out_file: &PathBuf,
    script_dir: &PathBuf,
) -> Result<RmsdRmsf> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    if exists(&out_file) {
        info!("RMSD/RMSF file exists");
    } else {
        info!("Creating RMSD/RMSF file");
        let script = script_dir.join("get_rmsd_rmsf.py");
        let cmd = Command::new(&uv)
            .current_dir(&script_dir)
            .args([
                "run",
                &script.to_string_lossy().to_string(),
                "--out-file",
                &out_file.to_string_lossy().to_string(),
                "--structure",
                &min_pdb.to_string_lossy().to_string(),
                "--trajectory",
                &min_xtc.to_string_lossy().to_string(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?.to_string());

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !exists(&out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    let contents = fs::read_to_string(&out_file)?;
    let vals: RmsdRmsf = serde_json::from_str(&contents)?;

    Ok(vals)
}

// --------------------------------------------------
fn get_sequence(
    full_pdb: &PathBuf,
    sequence_file: &PathBuf,
    script_dir: &PathBuf,
) -> Result<ProteinSequence> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    if exists(&sequence_file) {
        info!("Sequence file exists");
    } else {
        info!("Creating sequence file");
        let script = script_dir.join("get_sequence_from_pdb.py");
        let cmd = Command::new(&uv)
            .current_dir(&script_dir)
            .args([
                "run",
                &script.to_string_lossy().to_string(),
                "--out-file",
                &sequence_file.to_string_lossy().to_string(),
                &full_pdb.to_string_lossy().to_string(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?.to_string());

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !exists(&sequence_file) {
            bail!(r#"Failed to create "{}""#, sequence_file.display());
        }
    }

    let contents = fs::read_to_string(&sequence_file)?;
    let sequence: ProteinSequence = serde_json::from_str(&contents)?;

    Ok(sequence)
}

// --------------------------------------------------
fn sample_trajectory(
    min_xtc: &PathBuf,
    min_pdb: &PathBuf,
    sampled_trajectory: &PathBuf,
    script_dir: &PathBuf,
) -> Result<()> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
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

    Ok(())
}

// --------------------------------------------------
fn make_import_json(
    files: &FullMinFiles,
    sampled_trajectory: &PathBuf,
    thumbnail: &PathBuf,
    import_json: &PathBuf,
    script_dir: &PathBuf,
) -> Result<()> {
    if exists(import_json) {
        info!("Import JSON exists");
    } else {
        info!("Creating import JSON");
    }
    Ok(())
}

// --------------------------------------------------
fn get_duration(full_xtc: &PathBuf, out_file: &PathBuf) -> Result<Duration> {
    if exists(&out_file) {
        info!("Duration file exists");
    } else {
        info!("Creating duration file");
        let cmd = Command::new("molly")
            .args(["--info", &full_xtc.to_string_lossy().to_string()])
            .output()?;

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        let stdout = str::from_utf8(&cmd.stdout)?.to_string();
        let time_re = Regex::new(r"^time:\s*(\d+)-(\d+)\s+ps").unwrap();
        let nframes_re = Regex::new(r"^nframes:\s*(\d+)").unwrap();
        let mut time_start: Option<u64> = None;
        let mut time_stop: Option<u64> = None;
        let mut num_frames: Option<u64> = None;
        for line in stdout.split("\n") {
            println!("{line}");
            if let Some(caps) = time_re.captures(line) {
                let start = caps.get(1).unwrap().as_str();
                let stop = caps.get(2).unwrap().as_str();

                if let Ok(tmp) = start.parse::<u64>() {
                    time_start = Some(tmp);
                } else {
                    info!("Failed to parse time_start from \"{start}\" ({line})")
                }

                if let Ok(tmp) = stop.parse::<u64>() {
                    time_stop = Some(tmp);
                } else {
                    info!("Failed to parse time_start from \"{stop}\" ({line})")
                }
            } else if let Some(caps) = nframes_re.captures(line) {
                let val = caps.get(1).unwrap().as_str();
                if let Ok(tmp) = val.parse::<u64>() {
                    num_frames = Some(tmp);
                } else {
                    info!("Failed to parse num_frames from \"{val}\" ({line})")
                }
            }
        }
        if [time_start, time_stop, num_frames]
            .iter()
            .any(|v| v.is_none())
        {
            bail!("Failed to parse molly output:\n{stdout}")
        }

        let time_start = time_start.unwrap() as f64;
        let time_stop = time_stop.unwrap() as f64;
        let num_frames = num_frames.unwrap() as f64;

        if num_frames <= 1. {
            bail!("Trajectory file has only {num_frames} frame(s)");
        }

        {
            let totaltime_ns = ((time_stop - time_start) / 1000.0).round();
            let sampling_frequency_ns = format!("{:.2}", totaltime_ns / num_frames)
                .parse::<f32>()
                .unwrap();
            let mut fh = File::create(&out_file)?;
            let duration = Duration {
                totaltime_ns: totaltime_ns as u32,
                sampling_frequency_ns,
            };
            let json = serde_json::to_string_pretty(&duration)?;
            writeln!(&mut fh, "{json}")?;
        }

        if !exists(&out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    let contents = fs::read_to_string(&out_file)?;
    let duration: Duration = serde_json::from_str(&contents)?;

    Ok(duration)
}

// --------------------------------------------------
fn get_topology_hash(topology: &PathBuf) -> Result<String> {
    let contents = fs::read(&topology)?;
    let digest = Sha1::digest(&contents);
    Ok(format!("{digest:x}"))
}

// --------------------------------------------------
fn get_uniprot_entry(uniprot_id: &str) -> Result<UniprotEntry> {
    let url = format!("https://rest.uniprot.org/uniprotkb/{uniprot_id}.json");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!("Failed to GET \"{url}\" ({})", resp.status());
    }
    let uniprot: UniprotResponse = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse Uniprot response: {e}"))?;

    let desc = uniprot.protein_description;
    let name = if let Some(name) = desc.recommended_name {
        name.full_name.value
    } else if let Some(name) = desc.submission_names {
        name.full_name.value
    } else {
        bail!("Uniprot entry for \"{uniprot_id}\" has no names")
    };

    Ok(UniprotEntry {
        uniprot_id: uniprot_id.to_string(),
        name,
        sequence: uniprot.sequence.value,
    })
}

// --------------------------------------------------
fn get_pdb_entry(pdb_id: &str) -> Result<PdbEntry> {
    let url = format!(
        "https://data.rcsb.org/rest/v1/core/entry/{}",
        pdb_id.to_uppercase()
    );
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!("Failed to GET \"{url}\" ({})", resp.status());
    }
    let pdb_resp: PdbResponse = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse PDB response: {e}"))?;
    dbg!(&pdb_resp);

    Ok(PdbEntry {
        pdb_id: pdb_id.to_string(),
        title: pdb_resp.struct_.title.to_string(),
        classification: pdb_resp.struct_keywords.pdbx_keywords.to_string(),
        uniprots: vec![],
    })
}
