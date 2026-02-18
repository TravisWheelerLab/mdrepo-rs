use crate::{
    common::{file_exists, get_md5},
    metadata::{Meta, MoleculeType},
    types::{
        Duration, Export, MdContributor, MdFile, MdLigand, MdPaper, MdSimulation,
        MdSoftware, MdSolvent, PdbEntry, PdbGraphqlResponse, PdbResponse, ProcessArgs,
        ProcessedFiles, RmsdRmsf, UniprotEntry, UniprotResponse,
    },
};
use anyhow::{anyhow, bail, Result};
//use dotenvy::dotenv;
use log::{debug, info};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File},
    io::Write,
    path::{self, PathBuf},
    process::Command,
};
use which::which;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<()> {
    debug!("{args:?}");
    let input_dir = path::absolute(&args.dirname)?;
    let processed_dir = args
        .out_dir
        .clone()
        .map_or(input_dir.join("processed"), |dir| PathBuf::from(&dir));
    let script_dir = &args.script_dir.clone().unwrap();
    debug!("{processed_dir:?}");

    let meta_path = input_dir.join("mdrepo-metadata.toml");
    let processed_files =
        make_processed_files(&meta_path, &input_dir, &processed_dir, &script_dir)?;

    let json_dir = &args.json_dir.clone().unwrap();
    if !json_dir.is_dir() {
        fs::create_dir_all(&json_dir)?;
    }
    let in_dir_basename = &input_dir.file_name().unwrap().to_string_lossy().to_string();
    let import_json = json_dir.join(format!("{in_dir_basename}.json"));

    make_import_json(
        &meta_path,
        &input_dir,
        &script_dir,
        &processed_files,
        &import_json,
        None,
    )?;

    Ok(())
}

// --------------------------------------------------
pub fn make_thumbnail(
    thumbnail: &PathBuf,
    sampled_trajectory: &PathBuf,
    min_pdb: &PathBuf,
    script_dir: &PathBuf,
) -> Result<()> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    if file_exists(&thumbnail) {
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

        if !file_exists(&thumbnail) {
            bail!(r#"Failed to create "{}""#, thumbnail.display());
        }
    }
    Ok(())
}

// --------------------------------------------------
pub fn make_processed_files(
    meta_path: &PathBuf,
    in_dir: &PathBuf,
    processed_dir: &PathBuf,
    script_dir: &PathBuf,
) -> Result<ProcessedFiles> {
    let meta = Meta::from_file(&meta_path)?;
    let reqd_file = &meta.required_files;
    let full_min_files = &[
        "full.gro",
        "full.pdb",
        "full.xtc",
        "minimal.gro",
        "minimal.pdb",
        "minimal.xtc",
    ]
    .map(|f| processed_dir.join(f));

    if full_min_files.iter().all(file_exists) {
        info!("Full/minimal files all exist");
    } else {
        let micromamba = which("micromamba")
            .map_err(|e| anyhow!("Failed to find micromamba ({e})"))?;
        info!("Making full/minimal files");
        let cpp_traj = &script_dir.join("cpptraj_gmx_traj_manipulation.py");
        if !cpp_traj.is_file() {
            bail!(r#"Missing "{}""#, cpp_traj.display());
        }
        let mut cmd = Command::new(micromamba);
        cmd.args([
            "run",
            "-n",
            "simproc",
            &cpp_traj.to_string_lossy().to_string(),
            "--traj",
            &in_dir
                .join(&reqd_file.trajectory_file_name)
                .to_string_lossy()
                .to_string(),
            "--coord",
            &in_dir
                .join(&reqd_file.structure_file_name)
                .to_string_lossy()
                .to_string(),
            "--top",
            &in_dir
                .join(&reqd_file.topology_file_name)
                .to_string_lossy()
                .to_string(),
            "--outdir",
            &processed_dir.to_string_lossy().to_string(),
        ]);
        debug!("{cmd:?}");
        let output = cmd.output()?;
        debug!("{output:?}");

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
        }

        let missing: Vec<_> = full_min_files
            .iter()
            .filter(|f| !file_exists(f))
            .map(|f| f.to_string_lossy().to_string())
            .collect();

        if !missing.is_empty() {
            bail!("Failed to create: {}", missing.join(", "));
        }
    }

    let full_gro = processed_dir.join("full.gro");
    let full_pdb = processed_dir.join("full.pdb");
    let full_xtc = processed_dir.join("full.xtc");
    let min_gro = processed_dir.join("minimal.gro");
    let min_pdb = processed_dir.join("minimal.pdb");
    let min_xtc = processed_dir.join("minimal.xtc");
    let sampled_xtc = processed_dir.join("sampled.xtc");
    let thumbnail_png = processed_dir.join("thumbnail.png");

    sample_trajectory(&min_xtc, &min_pdb, &sampled_xtc, &script_dir)?;
    make_thumbnail(&thumbnail_png, &sampled_xtc, &min_pdb, &script_dir)?;

    Ok(ProcessedFiles {
        full_gro,
        full_pdb,
        full_xtc,
        min_gro,
        min_pdb,
        min_xtc,
        sampled_xtc,
        thumbnail_png,
    })
}

// --------------------------------------------------
pub fn get_rmsd_rmsf(
    min_pdb: &PathBuf,
    min_xtc: &PathBuf,
    script_dir: &PathBuf,
) -> Result<RmsdRmsf> {
    let processed_dir = min_pdb.parent().unwrap();
    let out_file = processed_dir.join("rmsd_rmsf.json");

    if file_exists(&out_file) {
        info!("RMSD/RMSF file exists");
    } else {
        info!("Creating RMSD/RMSF file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
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

        if !file_exists(&out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    let contents = fs::read_to_string(&out_file)?;
    let vals: RmsdRmsf = serde_json::from_str(&contents)?;

    Ok(vals)
}

// --------------------------------------------------
pub fn get_sequence(full_pdb: &PathBuf, script_dir: &PathBuf) -> Result<String> {
    let processed_dir = full_pdb.parent().unwrap();
    let sequence_file = processed_dir.join("sequence.fa");

    if file_exists(&sequence_file) {
        info!("Sequence file exists");
    } else {
        info!("Creating sequence file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
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

        if !file_exists(&sequence_file) {
            bail!(r#"Failed to create "{}""#, sequence_file.display());
        }
    }

    fs::read_to_string(&sequence_file)
        .map_err(|e| anyhow!("{}: {e}", sequence_file.display()))
}

// --------------------------------------------------
pub fn sample_trajectory(
    min_xtc: &PathBuf,
    min_pdb: &PathBuf,
    out_file: &PathBuf,
    script_dir: &PathBuf,
) -> Result<()> {
    if file_exists(&out_file) {
        info!("Sampled trajectory exists");
    } else {
        info!("Creating sampled trajectory");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
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
                &out_file.to_string_lossy().to_string(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?.to_string());

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !file_exists(&out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    Ok(())
}

// --------------------------------------------------
//pub fn db_connection(server: &Server) -> Result<postgres::Client> {
//    dotenv().ok();
//    let env_key = match server {
//        Server::Production => "PRODUCTION_DB_URL",
//        Server::Staging => "STAGING_DB_URL",
//    };
//    dbg!(&env_key);
//    let db_url = env::var(env_key).expect(&format!("{env_key} must be set"));
//    postgres::Client::connect(&db_url, postgres::NoTls)
//        .map_err(|_| anyhow!("Failed db connection"))
//}

// --------------------------------------------------
pub fn make_import_json(
    meta_path: &PathBuf,
    input_dir: &PathBuf,
    script_dir: &PathBuf,
    processed_files: &ProcessedFiles,
    //server: &Server,
    import_json: &PathBuf,
    simulation_id: Option<u32>,
) -> Result<()> {
    let meta = Meta::from_file(&meta_path)?;

    //let mut dbh = db_connection(server)?;
    //println!("Connected to {server:?}");

    let topology_path = input_dir.join(&meta.required_files.topology_file_name);
    let topology_hash = get_topology_hash(&topology_path)?;
    //dbg!(&topology_hash);

    let fasta_sequence = get_sequence(&processed_files.full_pdb, &script_dir)?;
    //dbg!(&sequence);

    let rmsd_rmsf = get_rmsd_rmsf(
        &processed_files.min_pdb,
        &processed_files.min_xtc,
        &script_dir,
    )?;
    //dbg!(&rmsd_rmsf);

    let integration_timestep_fs = match &meta.timestep_information {
        Some(val) => val.integration_time_step.unwrap(),
        _ => bail!("Missing timestep"),
    };

    let duration = get_duration(&processed_files.full_xtc, integration_timestep_fs)?;
    //dbg!(&duration);

    //let sim_id = find_or_create_simulation(
    //    &mut dbh,
    //    &unique_file_hash,
    //    &meta,
    //    &sequence,
    //    &topology_hash,
    //    &duration,
    //    &rmsd_rmsf,
    //)?;
    //dbg!(&sim_id);

    //dbg!(&meta.proteins);
    let mut uniprots: Vec<UniprotEntry> = vec![];
    let mut pdbs: Vec<PdbEntry> = vec![];
    for protein in &meta.proteins {
        match (
            protein.molecule_id_type.clone(),
            protein.molecule_id.clone(),
        ) {
            (Some(MoleculeType::Uniprot), Some(uniprot_id)) => {
                if !uniprot_present(&uniprot_id, &uniprots) {
                    match get_uniprot_entry(&uniprot_id) {
                        Ok(uniprot) => uniprots.push(uniprot),
                        _ => info!(r#"Failed to get Uniprot entry for "{uniprot_id}""#),
                    }
                }
            }
            (Some(MoleculeType::PDB), Some(pdb_id)) => match get_pdb_entry(&pdb_id) {
                Ok((pdb, pdb_uniprots)) => {
                    if !pdb_present(&pdb.pdb_id, &pdbs) {
                        pdbs.push(pdb);
                    }
                    for uniprot in pdb_uniprots {
                        if !uniprot_present(&uniprot.uniprot_id, &uniprots) {
                            uniprots.push(uniprot);
                        }
                    }
                }
                _ => info!(r#"Failed to get PDB entry for "{pdb_id}""#),
            },
            _ => println!("Handle {protein:?}"),
        }
    }

    if let Some(pdb_id) = &meta.pdb_id {
        let (pdb, pdb_uniprots) = get_pdb_entry(&pdb_id)?;
        if !pdb_present(&pdb.pdb_id, &pdbs) {
            pdbs.push(pdb);
        }

        for uniprot in pdb_uniprots {
            if !uniprot_present(&uniprot.uniprot_id, &uniprots) {
                uniprots.push(uniprot);
            }
        }
    }

    if pdbs.len() > 1 {
        bail!("There cannot be {} PDBs!", pdbs.len());
    }

    //dbg!(&uniprots);
    //dbg!(&pdbs);

    let (forcefield, forcefield_comments) = match &meta.forcefield {
        Some(val) => (val.forcefield.clone(), val.forcefield_comments.clone()),
        _ => (None, None),
    };

    let protonation_method = match &meta.protonation_method {
        Some(val) => val.protonation_method.clone(),
        _ => None,
    };

    let temperature = match &meta.temperature {
        Some(val) => val.temperature.clone(),
        _ => None,
    };

    let (includes_water, water_type, water_density, water_density_units) =
        match &meta.water {
            Some(val) => (
                val.is_present.unwrap_or(true),
                val.model.clone(),
                val.density,
                val.water_density_units.clone(),
            ),
            _ => (false, None, None, None),
        };

    let (replicate, total_replicates) = match &meta.replicates {
        Some(val) => (
            val.replicate.unwrap_or(1),
            val.total_replicates.unwrap_or(1),
        ),
        _ => bail!("Missing replicates"),
    };

    let software = MdSoftware {
        name: meta.software.name.clone(),
        version: meta.software.version.clone(),
    };

    let contributors = match &meta.contributors {
        Some(vals) => vals
            .iter()
            .enumerate()
            .map(|(rank, val)| MdContributor {
                name: val.name.clone(),
                orcid: val.orcid.clone(),
                institution: val.institution.clone(),
                email: val.email.clone(),
                rank: (rank + 1) as u32,
            })
            .collect::<Vec<_>>(),
        _ => vec![],
    };

    let mut original_files: Vec<MdFile> = vec![MdFile {
        name: meta_path.file_name().unwrap().to_string_lossy().to_string(),
        file_type: "Metadata".to_string(),
        size: meta_path.metadata()?.len(),
        md5_sum: get_md5(meta_path)?,
        description: None,
    }];

    for (file_type, filename) in &[
        ("Trajectory", &meta.required_files.trajectory_file_name),
        ("Structure", &meta.required_files.structure_file_name),
        ("Topology", &meta.required_files.topology_file_name),
    ] {
        let path = input_dir.join(filename);
        original_files.push(MdFile {
            name: filename.to_string(),
            file_type: file_type.to_string(),
            size: path.metadata()?.len(),
            md5_sum: get_md5(&path)?,
            description: None,
        })
    }

    if let Some(files) = &meta.additional_files {
        for file in files {
            let path = input_dir.join(&file.file_name);
            let md5_sum = get_md5(&path)?;
            if original_files
                .iter()
                .filter(|f| f.md5_sum == md5_sum)
                .collect::<Vec<_>>()
                .len()
                == 0
            {
                original_files.push(MdFile {
                    name: file.file_name.to_string(),
                    file_type: file.file_type.to_string(),
                    size: path.metadata()?.len(),
                    md5_sum,
                    description: file.description.clone(),
                })
            }
        }
    }

    let mut processed_export: Vec<MdFile> = vec![];
    for (file_type, path) in &[
        ("Processed topology", &processed_files.full_gro),
        ("Processed structure", &processed_files.full_pdb),
        ("Processed trajectory", &processed_files.full_xtc),
        ("Minimal topology", &processed_files.min_gro),
        ("Minimal structure", &processed_files.min_pdb),
        ("Minimal trajectory", &processed_files.min_xtc),
        ("Sampled minimal trajectory", &processed_files.sampled_xtc),
        ("Preview image", &processed_files.thumbnail_png),
    ] {
        processed_export.push(MdFile {
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            file_type: file_type.to_string(),
            size: path.metadata()?.len(),
            md5_sum: get_md5(&path)?,
            description: None,
        })
    }

    let mut ligands = vec![];
    if let Some(vals) = &meta.ligands {
        for ligand in vals {
            ligands.push(MdLigand {
                name: ligand.name.clone(),
                smiles: ligand.smiles.clone(),
            });
        }
    }

    let mut solvents = vec![];
    if let Some(vals) = &meta.solvents {
        for solvent in vals {
            solvents.push(MdSolvent {
                name: solvent.name.clone(),
                concentration: solvent.ion_concentration.clone(),
                concentration_units: solvent
                    .concentration_units
                    .clone()
                    .map_or("mol/L".to_string(), |val| val.to_string()),
            });
        }
    }

    let mut papers = vec![];
    if let Some(vals) = &meta.papers {
        for paper in vals {
            papers.push(MdPaper {
                title: paper.title.clone(),
                authors: paper.authors.clone(),
                journal: paper.journal.clone(),
                volume: paper.volume.to_integer().unwrap(),
                number: paper.number.clone().map(|val| val.to_string().unwrap()),
                year: paper.year as i64,
                pages: paper.pages.clone(),
                doi: paper.doi.clone(),
            })
        }
    }

    let initial = meta.initial.clone();
    let simulation = MdSimulation {
        simulation_id,
        lead_contributor_orcid: initial.lead_contributor_orcid,
        unique_file_hash_string: unique_file_hash(&meta, input_dir),
        description: initial
            .description
            .map_or("".to_string(), |val| val.to_string()),
        short_description: initial.short_description.clone(),
        run_commands: initial.commands.clone(),
        software,
        pdb: pdbs.first().map(|val| val.clone()),
        uniprots,
        duration: duration.totaltime_ns,
        sampling_frequency: duration.sampling_frequency_ns,
        integration_timestep_fs,
        external_link: initial.external_link.clone(),
        forcefield,
        forcefield_comments,
        protonation_method,
        rmsd_values: rmsd_rmsf.rmsd,
        rmsf_values: rmsd_rmsf.rmsf,
        temperature,
        fasta_sequence,
        replicate,
        total_replicates,
        includes_water,
        water_density,
        water_density_units,
        water_type,
        topology_hash,
        contributors,
        original_files,
        processed_files: processed_export,
        ligands,
        solvents,
        papers,
    };

    let export = Export { simulation };

    info!(r#"Writing JSON to "{}""#, &import_json.display());
    let file = File::create(&import_json)?;
    writeln!(&file, "{}", &serde_json::to_string_pretty(&export)?)?;

    Ok(())
}

// --------------------------------------------------
pub fn get_duration(
    full_xtc: &PathBuf,
    integration_timestep_fs: f64,
) -> Result<Duration> {
    let processed_dir = full_xtc.parent().unwrap();
    let out_file = processed_dir.join("duration.json");

    if file_exists(&out_file) {
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
        let time_re = Regex::new(r"^time:\s*(\d+)-(\d+(?:\.\d)?)\s+ps").unwrap();
        let nframes_re = Regex::new(r"^nframes:\s*(\d+)").unwrap();
        let mut time_start: Option<u64> = None;
        let mut time_stop: Option<u64> = None;
        let mut num_frames: Option<u64> = None;
        for line in stdout.split("\n") {
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
                } else if let Ok(tmp) = stop.parse::<f64>() {
                    time_stop = format!("{}", tmp.round()).parse::<u64>().ok();
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

        // time_start/time_stop are in ps from molly
        let mut duration_ps = time_stop - time_start;
        let sampling_ps = duration_ps / (num_frames - 1.0);

        // Sanity check: compute nstxout (output steps per frame)
        // using the integration timestep from metadata.
        let dt_ps = integration_timestep_fs / 1000.0;
        let nstxout = sampling_ps / dt_ps;

        // A reasonable nstxout is 1e3..1e7. If it's way too large
        // but dividing by 1000 fixes it, the XTC timestamps are
        // inflated by 1000x (a known issue with some MD engines).
        if nstxout > 1e7 {
            let corrected_nstxout = nstxout / 1000.0;
            if corrected_nstxout >= 1e3 && corrected_nstxout <= 1e7 {
                info!(
                    "XTC timestamps appear inflated by 1000x \
                         (nstxout={nstxout:.0}, corrected={corrected_nstxout:.0}). \
                         Applying correction."
                );
                duration_ps /= 1000.0;
            } else {
                bail!(
                    "XTC timestamps look wrong (nstxout={nstxout:.0}) \
                         but no clean 1000x correction found"
                );
            }
        }

        let totaltime_ns = (duration_ps / 1000.0).round();
        let sampling_frequency_ns = format!("{:.2}", totaltime_ns / num_frames)
            .parse::<f32>()
            .unwrap();
        let duration = Duration {
            totaltime_ns: totaltime_ns as u32,
            sampling_frequency_ns,
        };

        // Scoped to force close of fh
        {
            let json = serde_json::to_string_pretty(&duration)?;
            let mut fh = File::create(&out_file)?;
            writeln!(&mut fh, "{json}")?;
        }

        if !file_exists(&out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    let contents = fs::read_to_string(&out_file)?;
    let duration: Duration = serde_json::from_str(&contents)?;

    Ok(duration)
}

// --------------------------------------------------
pub fn get_topology_hash(topology: &PathBuf) -> Result<String> {
    let contents = fs::read(&topology)?;
    let digest = Sha1::digest(&contents);
    Ok(format!("{digest:x}"))
}

// --------------------------------------------------
pub fn get_uniprot_entry(uniprot_id: &str) -> Result<UniprotEntry> {
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
pub fn get_pdb_entry(pdb_id: &str) -> Result<(PdbEntry, Vec<UniprotEntry>)> {
    let pdb_id = pdb_id.to_uppercase();
    let url = format!("https://data.rcsb.org/rest/v1/core/entry/{pdb_id}");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!("Failed to GET \"{url}\" ({})", resp.status());
    }
    let pdb_resp: PdbResponse = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse PDB response: {e}"))?;

    let query = [
        "{entry(entry_id:\"",
        &pdb_id,
        "\"){polymer_entities{uniprots{rcsb_id,rcsb_uniprot_protein{",
        "name{value},sequence}}}}}",
    ]
    .join("");
    let url = format!(
        "https://data.rcsb.org/graphql?query={}",
        urlencoding::encode(&query)
    );

    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!("Failed to GET \"{url}\" ({})", resp.status());
    }
    let graphql_resp: PdbGraphqlResponse = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse PDB response: {e}"))?;

    let mut uniprots: Vec<UniprotEntry> = vec![];
    for entry in graphql_resp.data.entry.polymer_entities {
        for uniprot in entry.uniprots {
            uniprots.push(UniprotEntry {
                uniprot_id: uniprot.rcsb_id,
                name: uniprot.rcsb_uniprot_protein.name.value,
                sequence: uniprot.rcsb_uniprot_protein.sequence,
            })
        }
    }

    Ok((
        PdbEntry {
            pdb_id: pdb_id.to_string(),
            title: pdb_resp.struct_.title.to_string(),
            classification: pdb_resp.struct_keywords.pdbx_keywords.to_string(),
        },
        uniprots,
    ))
}

// --------------------------------------------------
//fn find_or_create_simulation(
//    dbh: &mut postgres::Client,
//    unique_file_hash: &str,
//    _meta: &Meta,
//    _sequence: &ProteinSequence,
//    topology_hash: &str,
//    _duration: &Duration,
//    _rmsd_rmsf: &RmsdRmsf,
//) -> Result<u64> {
//    let replicate_group_id = find_or_create_replicate_group(dbh, topology_hash)?;
//    dbg!(&replicate_group_id);

//    let res = dbh.query(
//        "select id from md_simulation where unique_file_hash_string=$1",
//        &[&unique_file_hash],
//    )?;

//    if res.is_empty() {}
//    dbg!(&res);
//    Ok(0)
//}

// --------------------------------------------------
//fn find_or_create_replicate_group(
//    dbh: &mut postgres::Client,
//    topology_hash: &str,
//) -> Result<i64> {
//    let res = dbh.query(
//        "select id from md_simulation_replicate_group where psf_hash=$1",
//        &[&topology_hash],
//    )?;

//    if let Some(first) = res.first() {
//        return Ok(first.get::<usize, i64>(0));
//    }

//    let res = dbh.query(
//        "
//        insert
//        into   md_simulation_replicate_group (psf_hash)
//        values ($1)
//        returning id;
//        ",
//        &[&topology_hash],
//    )?;

//    if let Some(first) = res.first() {
//        return Ok(first.get::<usize, i64>(0));
//    }

//    Ok(0)
//}

// --------------------------------------------------
pub fn unique_file_hash(meta: &Meta, input_dir: &PathBuf) -> String {
    let mut input_files = vec![
        meta.required_files.trajectory_file_name.to_string(),
        meta.required_files.structure_file_name.to_string(),
        meta.required_files.topology_file_name.to_string(),
    ];

    if let Some(addl_files) = &meta.additional_files {
        for file in addl_files {
            input_files.push(file.file_name.to_string());
        }
    }

    let mut md5s: Vec<String> = input_files
        .iter()
        .filter_map(|filename| get_md5(&input_dir.join(filename)).ok())
        .collect();

    md5s.sort();

    md5s.join(",")
}

// --------------------------------------------------
fn uniprot_present(uniprot_id: &str, uniprots: &Vec<UniprotEntry>) -> bool {
    let ids: Vec<_> = uniprots.iter().map(|u| u.uniprot_id.as_str()).collect();
    return ids.contains(&uniprot_id);
}

// --------------------------------------------------
fn pdb_present(pdb_id: &str, pdbs: &Vec<PdbEntry>) -> bool {
    let ids: Vec<_> = pdbs.iter().map(|p| p.pdb_id.as_str()).collect();
    return ids.contains(&pdb_id);
}
