use crate::types::{
    BlastResult, CheckedLigand, DoiPaper, Duration, Export, ExportSimulation,
    ImportResult, InferredLigand, MdFile, PdbEntry, PdbGraphqlResponse, PdbResponse,
    ProcessArgs, ProcessedFiles, PushResult, RmsdRmsf, UniprotEntry, UniprotResponse,
};
use anyhow::{anyhow, bail, Result};
use csv::ReaderBuilder;
use dotenvy::dotenv;
use libmdrepo::{
    common::{file_exists, get_md5, read_file},
    constants::{MOLLY_NFRAMES_REGEX, MOLLY_TIME_REGEX},
    metadata::{self, Meta, MetaCheckOptions},
};
use log::{debug, info};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufReader, Write},
    path::{self, Path, PathBuf},
    process::Command,
};
use which::which;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<()> {
    debug!("{args:?}");
    dotenv().ok();

    let input_dir = path::absolute(&args.input_dir)?;
    let processed_dir = args
        .out_dir
        .clone()
        .map_or(input_dir.join("processed"), |dir| PathBuf::from(&dir));
    let script_dir = &args.script_dir.clone().unwrap_or(PathBuf::from(
        env::var("SCRIPT_DIR").map_err(|e| anyhow!("SCRIPT_DIR: {e}"))?,
    ));
    let work_dir = &args.work_dir.clone().unwrap_or(PathBuf::from(
        env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
    ));
    let uniprot_blast_dir = work_dir.join("blast").join("uniprot");

    debug!(r#"Processed files will go to "{processed_dir:?}""#);
    if args.force && processed_dir.is_dir() {
        debug!("Removing processed directory");
        fs::remove_dir_all(&processed_dir)?;
    }

    let meta_path = input_dir.join("mdrepo-metadata.toml");
    let meta = Meta::from_file(&meta_path)?;
    let opts = if args.no_id {
        Some(MetaCheckOptions {
            allow_no_pdb_uniprot: true,
        })
    } else {
        None
    };
    let errors = meta.check(opts);
    if !errors.is_empty() {
        bail!(
            "Found {} error{} in mdrepo-metadata.toml:\n{}",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" },
            errors.join("\n")
        )
    }

    let processed_files =
        make_processed_files(&meta_path, &input_dir, &processed_dir, script_dir)?;

    let import_json = make_import_json(
        &meta_path,
        &input_dir,
        script_dir,
        &uniprot_blast_dir,
        &processed_files,
        args.simulation_id,
    )?;

    if !args.dry_run {
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let import_script = script_dir.join("import_preprocessed.py");
        info!(r#"Import "{}""#, import_json.display());
        let out_file = &processed_dir.join("imported.json");
        let mut cmd = Command::new(&uv);
        let mut import_args = vec![
            "run".to_string(),
            import_script.to_string_lossy().to_string(),
            "--file".to_string(),
            import_json.to_string_lossy().to_string(),
            "--data-dir".to_string(),
            input_dir.to_string_lossy().to_string(),
            "--server".to_string(),
            args.server.to_string(),
            "--out-file".to_string(),
            out_file.to_string_lossy().to_string(),
        ];
        if let Some(sim_id) = args.simulation_id {
            import_args.extend_from_slice(&[
                "--simulation-id".to_string(),
                sim_id.to_string(),
            ]);
        }
        cmd.current_dir(script_dir).args(&import_args);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
        }

        // The ticket directory should have been created by the fetch
        if !out_file.is_file() {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }

        let import_result: ImportResult = serde_json::from_str(&read_file(out_file)?)
            .map_err(|e| {
            anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display())
        })?;

        let push_script = script_dir.join("push_sim_files.py");
        info!(
            r#"Push files for "{}" -> simuation "{}""#,
            import_result.filename, import_result.simulation_id
        );

        let out_file = processed_dir.join("pushed.json");
        let mut cmd = Command::new(&uv);
        let mut push_args = vec![
            "run".to_string(),
            push_script.to_string_lossy().to_string(),
            "--file".to_string(),
            import_result.filename,
            "--simulation-id".to_string(),
            import_result.simulation_id.to_string(),
            "--server".to_string(),
            args.server.to_string(),
            "--data-dir".to_string(),
            input_dir.to_string_lossy().to_string(),
            "--out-file".to_string(),
            out_file.to_string_lossy().to_string(),
        ];

        // We should remove existing "processed" directory
        if args.simulation_id.is_some() {
            push_args.push("--remove-processed-dir".to_string());
        }

        cmd.current_dir(script_dir).args(&push_args);
        debug!("Running {cmd:?}");

        let output = cmd.output()?;
        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
        }

        if !out_file.is_file() {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }

        let push_res: Vec<PushResult> = serde_json::from_str(&read_file(&out_file)?)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display()))?;
        debug!("{push_res:?}");
    }

    Ok(())
}

// --------------------------------------------------
pub fn make_thumbnail(
    thumbnail: &Path,
    sampled_trajectory: &Path,
    min_pdb: &Path,
    script_dir: &Path,
) -> Result<()> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    if file_exists(thumbnail) {
        info!("Thumbnail exists");
    } else {
        info!("Creating thumbnail");
        let preview = script_dir.join("create_preview.py");
        let cmd = Command::new(&uv)
            .current_dir(script_dir)
            .args([
                "run",
                preview.to_string_lossy().as_ref(),
                "--trajectory",
                sampled_trajectory.to_string_lossy().as_ref(),
                "--structure",
                min_pdb.to_string_lossy().as_ref(),
                "--out-file",
                thumbnail.to_string_lossy().as_ref(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !file_exists(thumbnail) {
            bail!(r#"Failed to create "{}""#, thumbnail.display());
        }
    }
    Ok(())
}

// --------------------------------------------------
pub fn make_processed_files(
    meta_path: &Path,
    in_dir: &Path,
    processed_dir: &Path,
    script_dir: &Path,
) -> Result<ProcessedFiles> {
    let meta = Meta::from_file(meta_path)?;
    let full_min_files = &[
        "full.gro",
        "full.pdb",
        "full.xtc",
        "minimal.gro",
        "minimal.pdb",
        "minimal.xtc",
    ]
    .map(|f| processed_dir.join(f));

    if full_min_files.iter().all(|f| file_exists(f)) {
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
            cpp_traj.to_string_lossy().as_ref(),
            "--traj",
            in_dir
                .join(&meta.trajectory_file_name)
                .to_string_lossy()
                .as_ref(),
            "--coord",
            in_dir
                .join(&meta.structure_file_name)
                .to_string_lossy()
                .as_ref(),
            "--top",
            in_dir
                .join(&meta.topology_file_name)
                .to_string_lossy()
                .as_ref(),
            "--outdir",
            processed_dir.to_string_lossy().as_ref(),
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

    sample_trajectory(&min_xtc, &min_pdb, &sampled_xtc, script_dir)?;
    make_thumbnail(&thumbnail_png, &sampled_xtc, &min_pdb, script_dir)?;

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
    min_pdb: &Path,
    min_xtc: &Path,
    script_dir: &Path,
) -> Result<RmsdRmsf> {
    let processed_dir = min_pdb.parent().expect("parent");
    let out_file = processed_dir.join("rmsd_rmsf.json");

    if file_exists(&out_file) {
        info!("RMSD/RMSF file exists");
    } else {
        info!("Creating RMSD/RMSF file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let script = script_dir.join("get_rmsd_rmsf.py");
        let cmd = Command::new(&uv)
            .current_dir(script_dir)
            .args([
                "run",
                script.to_string_lossy().as_ref(),
                "--out-file",
                out_file.to_string_lossy().as_ref(),
                "--structure",
                min_pdb.to_string_lossy().as_ref(),
                "--trajectory",
                min_xtc.to_string_lossy().as_ref(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?);

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
pub fn blast_uniprot(
    fasta_sequence: &Path,
    uniprot_blast_dir: &Path,
) -> Result<Option<Vec<String>>> {
    if !uniprot_blast_dir.is_dir() {
        bail!(
            r#"Invalid Uniprot BLAST dir "{}""#,
            uniprot_blast_dir.display()
        );
    }

    let processed_dir = fasta_sequence.parent().expect("parent");
    let blast_results = processed_dir.join("blast.out");

    if file_exists(&blast_results) {
        info!("Uniprot BLAST results exists");
    } else {
        info!("Creating Uniprot BLAST results");
        let blastp =
            which("blastp").map_err(|e| anyhow!("Failed to find blastp ({e})"))?;
        let cmd = Command::new(&blastp)
            .args([
                "-query",
                fasta_sequence.to_string_lossy().as_ref(),
                "-db",
                uniprot_blast_dir
                    .join("uniprot_sprot")
                    .to_string_lossy()
                    .as_ref(),
                "-out",
                blast_results.to_string_lossy().as_ref(),
                "-outfmt",
                "6",
                "-evalue",
                "1e-5",
                "-num_threads",
                "4",
                "-max_target_seqs",
                "10",
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !file_exists(&blast_results) {
            bail!(r#"Failed to create "{}""#, blast_results.display());
        }
    }

    let file = BufReader::new(
        File::open(&blast_results)
            .map_err(|e| anyhow!("{}: {e}", blast_results.display()))?,
    );
    let mut reader = ReaderBuilder::new()
        .delimiter(9) // Tab
        .has_headers(false)
        .from_reader(file);

    let mut results = vec![];
    for result in reader.deserialize() {
        let hit: BlastResult = result?;
        // Just pick the top-scoring one, as long as it has >99% id
        if hit.pident >= 99.0 {
            results.push(hit.saccver.to_string());
            break;
        }
    }

    Ok(if results.is_empty() {
        None
    } else {
        Some(results)
    })
}

// --------------------------------------------------
pub fn get_sequence(full_pdb: &Path, script_dir: &Path) -> Result<PathBuf> {
    let processed_dir = full_pdb.parent().expect("parent");
    let sequence_file = processed_dir.join("sequence.fa");

    if file_exists(&sequence_file) {
        info!("Sequence file exists");
    } else {
        info!("Creating sequence file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let script = script_dir.join("get_sequence_from_pdb.py");
        let cmd = Command::new(&uv)
            .current_dir(script_dir)
            .args([
                "run",
                script.to_string_lossy().as_ref(),
                "--out-file",
                sequence_file.to_string_lossy().as_ref(),
                full_pdb.to_string_lossy().as_ref(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !file_exists(&sequence_file) {
            bail!(r#"Failed to create "{}""#, sequence_file.display());
        }
    }

    Ok(sequence_file)
}

// --------------------------------------------------
pub fn sample_trajectory(
    min_xtc: &Path,
    min_pdb: &Path,
    out_file: &Path,
    script_dir: &Path,
) -> Result<()> {
    if file_exists(out_file) {
        info!("Sampled trajectory exists");
    } else {
        info!("Creating sampled trajectory");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let sampler = script_dir.join("sample_trajectory.py");
        let cmd = Command::new(&uv)
            .current_dir(script_dir)
            .args([
                "run",
                sampler.to_string_lossy().as_ref(),
                "--trajectory",
                min_xtc.to_string_lossy().as_ref(),
                "--structure",
                min_pdb.to_string_lossy().as_ref(),
                "--outfile",
                out_file.to_string_lossy().as_ref(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !file_exists(out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    Ok(())
}

// --------------------------------------------------
pub fn make_import_json(
    meta_path: &Path,
    input_dir: &Path,
    script_dir: &Path,
    uniprot_blast_dir: &Path,
    processed_files: &ProcessedFiles,
    simulation_id: Option<u32>,
) -> Result<PathBuf> {
    let meta = Meta::from_file(meta_path)?;
    let topology_path = input_dir.join(&meta.topology_file_name);
    let topology_hash = get_topology_hash(&topology_path)?;
    let fasta_sequence_file = get_sequence(&processed_files.full_pdb, script_dir)?;
    let rmsd_rmsf = get_rmsd_rmsf(
        &processed_files.min_pdb,
        &processed_files.min_xtc,
        script_dir,
    )?;
    let duration =
        get_duration(&processed_files.full_xtc, meta.integration_timestep_fs)?;

    let inferred_ligands = get_inferred_ligands(
        &processed_files.min_pdb,
        &processed_files.min_gro,
        script_dir,
    )?;

    let unique_file_hash_string = get_unique_file_hash(&meta, input_dir);

    let mut ligands = vec![];
    let mut warnings = vec![];
    if let Some(given_ligands) = &meta.ligands {
        // For now, we'll still take all the given ligands
        ligands = given_ligands.clone();

        // But we'll check them against any inferred values
        if !inferred_ligands.is_empty() {
            for (ligand_num, given_ligand) in given_ligands.iter().enumerate() {
                let mut found_match = false;
                for inferred in &inferred_ligands {
                    let check = check_ligand(given_ligand, inferred, script_dir)?;
                    if !&[
                        check.exact_match,
                        check.same_connectivity,
                        check.same_connectivity_and_stereo,
                        check.same_inchi,
                    ]
                    .contains(&true)
                    {
                        found_match = true;
                        break;
                    }
                }

                if !found_match {
                    warnings.push(format!(
                        "Unable to verify ligand [{ligand_num}] ({})",
                        given_ligand.smiles
                    ));
                }
            }
        }
    } else {
        for ligand in inferred_ligands {
            let name = ligand
                .name
                .common_name
                .unwrap_or(ligand.name.iupac_name.unwrap_or("NA".to_string()));

            ligands.push({
                metadata::Ligand {
                    name,
                    smiles: ligand.structure.smiles,
                }
            })
        }
    }

    let mut uniprots: HashMap<String, UniprotEntry> = HashMap::new();
    if let Some(uniprot_ids) = meta
        .uniprot_ids
        .or(blast_uniprot(&fasta_sequence_file, uniprot_blast_dir)?)
    {
        for uniprot_id in uniprot_ids {
            if !uniprots.contains_key(&uniprot_id) {
                match get_uniprot_entry(&uniprot_id) {
                    Ok(entry) => {
                        let _ = uniprots.insert(uniprot_id.to_string(), entry);
                    }
                    _ => info!(r#"Failed to get Uniprot entry for "{uniprot_id}""#),
                }
            }
        }
    }

    let mut pdb = None;
    if let Some(pdb_id) = &meta.pdb_id {
        match get_pdb_entry(pdb_id) {
            Ok((pdb_tmp, _pdb_uniprots)) => {
                pdb = Some(pdb_tmp);

                // TODO: Verify that we will not do this.
                //for entry in pdb_uniprots {
                //    if !uniprots.contains_key(&entry.uniprot_id) {
                //        let _ = uniprots.insert(entry.uniprot_id.clone(), entry);
                //    }
                //}
            }
            Err(e) => info!("{e}"),
        }
    }

    let mut original_files: Vec<MdFile> = vec![MdFile {
        name: meta_path
            .file_name()
            .expect("filename")
            .to_string_lossy()
            .to_string(),
        file_type: "Metadata".to_string(),
        size: meta_path.metadata()?.len(),
        md5_sum: get_md5(meta_path)?,
        description: None,
        is_primary: None,
    }];

    for (file_type, filename) in &[
        ("Trajectory", &meta.trajectory_file_name),
        ("Structure", &meta.structure_file_name),
        ("Topology", &meta.topology_file_name),
    ] {
        let local_path = input_dir.join(filename);
        original_files.push(MdFile {
            name: filename.to_string(),
            file_type: file_type.to_string(),
            size: local_path.metadata()?.len(),
            md5_sum: get_md5(&local_path)?,
            description: None,
            is_primary: Some(true),
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
                .is_empty()
            {
                original_files.push(MdFile {
                    name: file.file_name.to_string(),
                    file_type: file.file_type.to_string(),
                    size: path.metadata()?.len(),
                    md5_sum,
                    description: file.description.clone(),
                    is_primary: None,
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
            name: path
                .file_name()
                .expect("filename")
                .to_string_lossy()
                .to_string(),
            file_type: file_type.to_string(),
            size: path.metadata()?.len(),
            md5_sum: get_md5(path)?,
            description: None,
            is_primary: None,
        })
    }

    let mut papers: Vec<metadata::Paper> = meta.papers.unwrap_or_default();
    if let Some(dois) = &meta.dois {
        for doi in dois {
            match get_doi(doi) {
                Ok(paper) => papers.push(paper),
                Err(e) => info!("{e}"),
            }
        }
    }

    let fasta_sequence = fs::read_to_string(fasta_sequence_file)?;
    let simulation = ExportSimulation {
        simulation_id,
        lead_contributor_orcid: meta.lead_contributor_orcid,
        unique_file_hash_string,
        user_accession: meta.user_accession,
        description: meta.description,
        short_description: meta.short_description,
        run_commands: meta.run_commands,
        software_name: meta.software_name,
        software_version: meta.software_version,
        pdb,
        uniprots: uniprots.into_values().collect::<Vec<_>>(),
        duration: duration.totaltime_ns,
        sampling_frequency: duration.sampling_frequency_ns,
        integration_timestep_fs: meta.integration_timestep_fs,
        external_links: meta.external_links.unwrap_or_default(),
        forcefield: meta.forcefield,
        forcefield_comments: meta.forcefield_comments,
        protonation_method: meta.protonation_method,
        rmsd_values: rmsd_rmsf.rmsd,
        rmsf_values: rmsd_rmsf.rmsf,
        temperature_kelvin: meta.temperature_kelvin,
        fasta_sequence,
        replicate_id: meta.replicate_id,
        water: meta.water,
        topology_hash,
        contributors: meta.contributors.unwrap_or_default(),
        original_files,
        processed_files: processed_export,
        ligands,
        solvents: meta.solvents.unwrap_or_default(),
        papers,
    };

    let export = Export {
        simulation,
        warnings,
    };

    let import_json = &input_dir.join("processed").join("import.json");
    info!(r#"Writing JSON to "{}""#, &import_json.display());
    let file = File::create(import_json)?;
    writeln!(&file, "{}", &serde_json::to_string_pretty(&export)?)?;

    Ok(import_json.into())
}

// --------------------------------------------------
pub fn check_ligand(
    ligand: &metadata::Ligand,
    inferred_ligand: &InferredLigand,
    script_dir: &Path,
) -> Result<CheckedLigand> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    let script = script_dir.join("compare_smiles.py");
    let cmd = Command::new(&uv)
        .current_dir(script_dir)
        .args([
            "run",
            script.to_string_lossy().as_ref(),
            &ligand.smiles,
            &inferred_ligand.structure.smiles,
        ])
        .output()?;

    if !cmd.status.success() {
        bail!(str::from_utf8(&cmd.stderr)?.to_string());
    }

    let stdout = str::from_utf8(&cmd.stdout)?;
    let checked = serde_json::from_str(stdout)?;
    Ok(checked)
}

// --------------------------------------------------
pub fn get_inferred_ligands(
    min_pdb: &Path,
    min_gro: &Path,
    script_dir: &Path,
) -> Result<Vec<InferredLigand>> {
    let processed_dir = min_pdb.parent().expect("parent");
    let out_file = processed_dir.join("inferred_ligands.json");
    if file_exists(&out_file) {
        info!("Inferred ligands file exists");
    } else {
        info!("Creating inferred ligands file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let mol_tools = script_dir.join("mol_tools.py");
        let cmd = Command::new(&uv)
            .current_dir(script_dir)
            .args([
                "run",
                mol_tools.to_string_lossy().as_ref(),
                "both",
                "--pdb",
                min_pdb.to_string_lossy().as_ref(),
                "--gro",
                min_gro.to_string_lossy().as_ref(),
                "--outfile",
                out_file.to_string_lossy().as_ref(),
            ])
            .output()?;

        info!("{}", str::from_utf8(&cmd.stdout)?);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        if !file_exists(&out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    let contents = fs::read_to_string(&out_file)?;
    let ligands: Vec<InferredLigand> = serde_json::from_str(&contents)?;

    Ok(ligands)
}

// --------------------------------------------------
pub fn get_duration(full_xtc: &Path, integration_timestep_fs: u32) -> Result<Duration> {
    let processed_dir = full_xtc
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", full_xtc.display()))?;
    let out_file = processed_dir.join("duration.json");

    if file_exists(&out_file) {
        info!("Duration file exists");
    } else {
        info!("Creating duration file");
        let cmd = Command::new("molly")
            .args(["--info", full_xtc.to_string_lossy().as_ref()])
            .output()?;

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }

        let stdout = str::from_utf8(&cmd.stdout)?.to_string();
        let mut time_start: Option<u64> = None;
        let mut time_stop: Option<u64> = None;
        let mut num_frames: Option<u64> = None;
        for line in stdout.split("\n") {
            if let Some(caps) = MOLLY_TIME_REGEX.captures(line) {
                let start = caps
                    .get(1)
                    .ok_or_else(|| anyhow!("Missing time start in: {line}"))?
                    .as_str();
                let stop = caps
                    .get(2)
                    .ok_or_else(|| anyhow!("Missing time stop in: {line}"))?
                    .as_str();

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
            } else if let Some(caps) = MOLLY_NFRAMES_REGEX.captures(line) {
                let val = caps
                    .get(1)
                    .ok_or_else(|| anyhow!("Missing nframes value in: {line}"))?
                    .as_str();
                if let Ok(tmp) = val.parse::<u64>() {
                    num_frames = Some(tmp);
                } else {
                    info!("Failed to parse num_frames from \"{val}\" ({line})")
                }
            }
        }
        let (time_start, time_stop, num_frames) = match (time_start, time_stop, num_frames) {
            (Some(a), Some(b), Some(c)) => (a as f64, b as f64, c as f64),
            _ => bail!("Failed to parse molly output:\n{stdout}"),
        };

        if num_frames <= 1. {
            bail!("Trajectory file has only {num_frames} frame(s)");
        }

        // time_start/time_stop are in ps from molly
        let mut duration_ps = time_stop - time_start;
        let sampling_ps = duration_ps / (num_frames - 1.0);

        // Sanity check: compute nstxout (output steps per frame)
        // using the integration timestep from metadata.
        let nstxout = sampling_ps / (integration_timestep_fs as f64 / 1000.0);

        // A reasonable nstxout is 1e3..1e7. If it's way too large
        // but dividing by 1000 fixes it, the XTC timestamps are
        // inflated by 1000x (a known issue with some MD engines).
        if nstxout > 1e7 {
            let corrected_nstxout = nstxout / 1000.0;
            if (1e3..=1e7).contains(&corrected_nstxout) {
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
            .map_err(|e| anyhow!("Failed to compute sampling frequency: {e}"))?;
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
pub fn get_topology_hash(topology: &Path) -> Result<String> {
    let contents = fs::read(topology)?;
    let digest = Sha1::digest(&contents);
    Ok(format!("{digest:x}"))
}

// --------------------------------------------------
pub fn get_doi(doi: &str) -> Result<metadata::Paper> {
    let url = format!("https://citation.doi.org/metadata?doi={doi}");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!(r#"Failed to GET "{url}" ({})""#, resp.status());
    }

    let doi_paper: DoiPaper = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse DOI response: {e}"))?;

    let authors: Vec<String> = doi_paper
        .author
        .iter()
        .map(|author| format!("{} {}", author.given, author.family))
        .collect();

    Ok(metadata::Paper {
        title: doi_paper.title.clone(),
        authors: authors.join(", "),
        journal: doi_paper.journal.clone(),
        volume: doi_paper.volume,
        number: None,
        year: *doi_paper.published.date_parts.first().expect("year"),
        pages: Some(doi_paper.page.clone()),
        doi: Some(doi.to_string()),
    })
}

// --------------------------------------------------
pub fn get_uniprot_entry(uniprot_id: &str) -> Result<UniprotEntry> {
    let url = format!("https://rest.uniprot.org/uniprotkb/{uniprot_id}.json");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!(r#"Failed to GET "{url}" ({})""#, resp.status());
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
pub fn get_unique_file_hash(meta: &Meta, input_dir: &Path) -> String {
    let mut input_files = vec![
        meta.trajectory_file_name.to_string(),
        meta.structure_file_name.to_string(),
        meta.topology_file_name.to_string(),
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
