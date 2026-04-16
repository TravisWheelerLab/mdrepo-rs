use crate::types::{
    BlastResult, CheckedLigand, DoiPaper, Duration, Export, ExportSimulation,
    ImportResult, InferredLigand, MdFile, PdbEntry, PdbResponse, ProcessArgs,
    ProcessedFiles, PushResult, RmsdRmsf, UniprotDb, UniprotEntry, UniprotResponse,
};
use anyhow::{anyhow, bail, Result};
use dotenvy::dotenv;
use libmdrepo::{
    common::{file_exists, get_md5, read_file},
    constants::{MOLLY_NFRAMES_REGEX, MOLLY_TIME_REGEX},
    metadata::{self, Meta, MetaCheckOptions},
};
use log::debug;
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    env,
    fs::{self, File},
    io::{BufReader, Write},
    mem,
    path::{self, Path, PathBuf},
    process::Command,
};
use which::which;

// ── BLAST parameters ──────────────────────────────────────────────────────────
const BLAST_EVALUE: &str = "1e-5";
const BLAST_NUM_THREADS: &str = "4";
const BLAST_MAX_TARGET_SEQS_SWISSPROT: &str = "20";
const BLAST_MAX_TARGET_SEQS_TREMBL: &str = "100";
const BLAST_MIN_PIDENT: f64 = 100.0;

// ── Unit conversions ──────────────────────────────────────────────────────────
const FS_PER_PS: f64 = 1000.0;
const PS_PER_NS: f64 = 1000.0;
const XTC_INFLATION_FACTOR: f64 = 1000.0;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<()> {
    debug!("{args:?}");
    dotenv().ok();

    let input_dir = path::absolute(&args.input_dir)?;
    let processed_dir = args
        .out_dir
        .clone()
        .map_or(input_dir.join("processed"), PathBuf::from);
    let script_dir = args.script_dir.clone().unwrap_or(PathBuf::from(
        env::var("SCRIPT_DIR").map_err(|e| anyhow!("SCRIPT_DIR: {e}"))?,
    ));
    let work_dir = args.work_dir.clone().unwrap_or(PathBuf::from(
        env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
    ));
    let blast_dir = work_dir.join("blast");

    debug!(r#"Processed files will go to "{processed_dir:?}""#);
    if args.force && processed_dir.is_dir() {
        debug!("Removing processed directory");
        fs::remove_dir_all(&processed_dir)?;
    }

    let meta_path = input_dir.join("mdrepo-metadata.toml");
    let meta = Meta::from_file(&meta_path)?;
    let errors = meta.check(if args.no_id {
        Some(MetaCheckOptions {
            allow_no_pdb_uniprot: true,
        })
    } else {
        None
    });
    if !errors.is_empty() {
        bail!(
            "Found {} error{} in mdrepo-metadata.toml:\n{}",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" },
            errors.join("\n")
        )
    }

    let processed_files =
        make_processed_files(&meta_path, &input_dir, &processed_dir, &script_dir)?;

    let import_json = make_import_json(
        &meta_path,
        &input_dir,
        &script_dir,
        &blast_dir,
        &processed_files,
        args.reprocess_simulation_id,
    )?;

    if !args.dry_run {
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let import_result = run_import(
            &uv,
            &script_dir,
            &import_json,
            &input_dir,
            &args.server.to_string(),
            args.reprocess_simulation_id,
            &processed_dir,
        )?;
        let push_res = run_push(
            &uv,
            &script_dir,
            import_result,
            &input_dir,
            &args.server.to_string(),
            args.reprocess_simulation_id,
            &processed_dir,
        )?;
        debug!("{push_res:?}");
    }

    Ok(())
}

// --------------------------------------------------
fn run_import(
    uv: &Path,
    script_dir: &Path,
    import_json: &Path,
    input_dir: &Path,
    server: &str,
    reprocess_simulation_id: Option<u32>,
    processed_dir: &Path,
) -> Result<ImportResult> {
    let import_script = script_dir.join("import_preprocessed.py");
    let out_file = processed_dir.join("imported.json");
    debug!(r#"Import "{}""#, import_json.display());

    let mut args = vec![
        "run".to_string(),
        import_script.to_string_lossy().to_string(),
        "--file".to_string(),
        import_json.to_string_lossy().to_string(),
        "--data-dir".to_string(),
        input_dir.to_string_lossy().to_string(),
        "--server".to_string(),
        server.to_string(),
        "--out-file".to_string(),
        out_file.to_string_lossy().to_string(),
    ];

    if let Some(id) = reprocess_simulation_id {
        args.extend(["--simulation-id".to_string(), id.to_string()]);
    }

    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir).args(&args);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(str::from_utf8(&output.stderr)?.to_string());
    }

    if !file_exists(&out_file) {
        bail!(r#"Failed to create "{}""#, out_file.display());
    }

    serde_json::from_str(&read_file(&out_file)?)
        .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display()))
}

// --------------------------------------------------
fn run_push(
    uv: &Path,
    script_dir: &Path,
    import_result: ImportResult,
    input_dir: &Path,
    server: &str,
    reprocess_simulation_id: Option<u32>,
    processed_dir: &Path,
) -> Result<Vec<PushResult>> {
    let push_script = script_dir.join("push_sim_files.py");
    let out_file = processed_dir.join("pushed.json");
    debug!(
        r#"Push files for "{}" -> simulation "{}""#,
        import_result.filename, import_result.simulation_id
    );

    let mut args = vec![
        "run".to_string(),
        push_script.to_string_lossy().to_string(),
        "--file".to_string(),
        import_result.filename,
        "--simulation-id".to_string(),
        import_result.simulation_id.to_string(),
        "--server".to_string(),
        server.to_string(),
        "--data-dir".to_string(),
        input_dir.to_string_lossy().to_string(),
        "--out-file".to_string(),
        out_file.to_string_lossy().to_string(),
    ];

    if reprocess_simulation_id.is_some() {
        args.push("--remove-processed-dir".to_string());
        //args.extend_from_slice(&[
        //    "--remove-processed-dir".to_string(),
        //    "--file-types".to_string(),
        //    "processed".to_string(),
        //    "media".to_string(),
        //]);
    }

    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir).args(&args);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(str::from_utf8(&output.stderr)?.to_string());
    }

    if !file_exists(&out_file) {
        let stdout = str::from_utf8(&output.stderr)?;
        bail!(r#"Failed to create "{}" ({stdout})"#, out_file.display());
    }

    serde_json::from_str(&read_file(&out_file)?)
        .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display()))
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
        debug!("Thumbnail exists");
    } else {
        debug!("Creating thumbnail");
        let preview = script_dir.join("create_preview.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            preview.to_string_lossy().as_ref(),
            "--trajectory",
            sampled_trajectory.to_string_lossy().as_ref(),
            "--structure",
            min_pdb.to_string_lossy().as_ref(),
            "--out-file",
            thumbnail.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
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
        debug!("Full/minimal files all exist");
    } else {
        let micromamba = which("micromamba")
            .map_err(|e| anyhow!("Failed to find micromamba ({e})"))?;
        debug!("Making full/minimal files");

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
        debug!("Running {cmd:?}");

        let output = cmd.output()?;
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
    let processed_dir = min_pdb
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", min_pdb.display()))?;
    let out_file = processed_dir.join("rmsd_rmsf.json");

    if file_exists(&out_file) {
        debug!("RMSD/RMSF file exists");
    } else {
        debug!("Creating RMSD/RMSF file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let script = script_dir.join("get_rmsd_rmsf.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            script.to_string_lossy().as_ref(),
            "--out-file",
            out_file.to_string_lossy().as_ref(),
            "--structure",
            min_pdb.to_string_lossy().as_ref(),
            "--trajectory",
            min_xtc.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
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
    blast_dir: &Path,
    uniprot_db: UniprotDb,
) -> Result<Vec<String>> {
    if !blast_dir.is_dir() {
        bail!(r#"Invalid BLAST dir "{}""#, blast_dir.display());
    }

    let processed_dir = fasta_sequence
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", fasta_sequence.display()))?;
    let blast_results = processed_dir.join(format!(
        "blast.{}.out",
        uniprot_db.to_string().to_lowercase()
    ));

    if file_exists(&blast_results) {
        fs::remove_file(&blast_results)?;
    }

    if file_exists(&blast_results) {
        debug!("Uniprot BLAST results exists");
    } else {
        debug!("Creating Uniprot BLAST results");
        let blastp =
            which("blastp").map_err(|e| anyhow!("Failed to find blastp ({e})"))?;

        let mut cmd = Command::new(&blastp);
        let (blast_db, max_target_seqs) = match uniprot_db {
            UniprotDb::Swissprot => (
                blast_dir.join("swissprot").join("uniprot_sprot"),
                BLAST_MAX_TARGET_SEQS_SWISSPROT,
            ),
            UniprotDb::Trembl => (
                blast_dir.join("trembl").join("uniprot_trembl.fasta"),
                BLAST_MAX_TARGET_SEQS_TREMBL,
            ),
        };

        cmd.args([
            "-query",
            fasta_sequence.to_string_lossy().as_ref(),
            "-db",
            blast_db.to_string_lossy().as_ref(),
            "-out",
            blast_results.to_string_lossy().as_ref(),
            "-outfmt",
            "6",
            "-evalue",
            BLAST_EVALUE,
            "-num_threads",
            BLAST_NUM_THREADS,
            "-max_target_seqs",
            max_target_seqs,
        ]);
        debug!("Running {cmd:?}");

        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
        }
    }

    let mut results = vec![];
    if file_exists(&blast_results) {
        let file = BufReader::new(
            File::open(&blast_results)
                .map_err(|e| anyhow!("{}: {e}", blast_results.display()))?,
        );

        let mut reader = csv::ReaderBuilder::new()
            .delimiter(9) // Tab
            .has_headers(false)
            .from_reader(file);

        let trembl_regex = Regex::new(r"^tr[|]([^|]+)[|]")?;
        for result in reader.deserialize() {
            let hit: BlastResult =
                result.map_err(|e| anyhow!("{}: {e}", blast_results.display()))?;
            if hit.pident >= BLAST_MIN_PIDENT {
                let subject = hit.saccver.to_string();
                match trembl_regex.captures(&subject) {
                    Some(caps) => {
                        let trembl_id = caps
                            .get(1)
                            .ok_or_else(|| anyhow!("regex capture group 1 not found"))?
                            .as_str();
                        results.push(trembl_id.to_string());
                    }
                    _ => results.push(subject),
                }
            }
        }
    }

    Ok(results)
}

// --------------------------------------------------
pub fn get_sequence(full_pdb: &Path, script_dir: &Path) -> Result<PathBuf> {
    let processed_dir = full_pdb
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", full_pdb.display()))?;
    let sequence_file = processed_dir.join("sequence.fa");

    if file_exists(&sequence_file) {
        debug!("Sequence file exists");
    } else {
        debug!("Creating sequence file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let script = script_dir.join("get_sequence_from_pdb.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            script.to_string_lossy().as_ref(),
            "--out-file",
            sequence_file.to_string_lossy().as_ref(),
            full_pdb.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
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
        debug!("Sampled trajectory exists");
    } else {
        debug!("Creating sampled trajectory");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let sampler = script_dir.join("sample_trajectory.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            sampler.to_string_lossy().as_ref(),
            "--trajectory",
            min_xtc.to_string_lossy().as_ref(),
            "--structure",
            min_pdb.to_string_lossy().as_ref(),
            "--outfile",
            out_file.to_string_lossy().as_ref(),
        ]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
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
    blast_dir: &Path,
    processed_files: &ProcessedFiles,
    reprocess_simulation_id: Option<u32>,
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

    let (uniprots, mut uniprot_warnings) =
        get_uniprot_entries(meta.uniprot_ids.clone(), &fasta_sequence_file, blast_dir)?;

    let mut ligands = vec![];
    let mut warnings = mem::take(&mut uniprot_warnings);
    if let Some(given_ligands) = &meta.ligands {
        // For now, we'll still take all the given ligands
        ligands = given_ligands.clone();

        // But we'll check them against any inferred values
        if !inferred_ligands.is_empty() {
            for (ligand_num, given_ligand) in given_ligands.iter().enumerate() {
                let mut found_match = false;
                for inferred in &inferred_ligands {
                    let check = check_ligand(given_ligand, inferred, script_dir)?;
                    if !(check.exact_match
                        || check.same_connectivity
                        || check.same_connectivity_and_stereo
                        || check.same_inchi)
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

    let mut pdb = None;
    if let Some(pdb_id) = &meta.pdb_id {
        match get_pdb_entry(pdb_id) {
            Ok(pdb_tmp) => {
                pdb = Some(pdb_tmp);
            }
            Err(e) => warnings.push(e.to_string()),
        }
    }

    // No need to push original files when reprocessing
    let mut original_files: Vec<MdFile> = vec![];
    if reprocess_simulation_id.is_none() {
        original_files.push(MdFile {
            name: meta_path
                .file_name()
                .ok_or_else(|| anyhow!("No filename for '{}'", meta_path.display()))?
                .to_string_lossy()
                .to_string(),
            file_type: "Metadata".to_string(),
            size: meta_path.metadata()?.len(),
            md5_sum: get_md5(meta_path)?,
            description: None,
            is_primary: None,
        });

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
                .ok_or_else(|| anyhow!("No filename for '{}'", path.display()))?
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
                Err(e) => debug!("{e}"),
            }
        }
    }

    let fasta_sequence = fs::read_to_string(fasta_sequence_file)?;
    let simulation = ExportSimulation {
        simulation_id: reprocess_simulation_id,
        lead_contributor_orcid: meta.lead_contributor_orcid,
        unique_file_hash_string,
        user_accession: meta.user_accession,
        description: meta.description,
        short_description: meta.short_description,
        run_commands: meta.run_commands,
        software_name: meta.software_name,
        software_version: meta.software_version,
        pdb,
        uniprots,
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

    if !warnings.is_empty() {
        let num_warnings = warnings.len();
        debug!(
            "{num_warnings} warning{}",
            if num_warnings == 1 { "" } else { "s" }
        );
        for (i, warning) in warnings.iter().enumerate() {
            debug!("{}: {warning}", i + 1);
        }
    }

    let export = Export {
        simulation,
        warnings,
    };

    let import_json = &input_dir.join("processed").join("import.json");
    debug!(r#"Writing JSON to "{}""#, &import_json.display());
    let file = File::create(import_json)?;
    writeln!(&file, "{}", &serde_json::to_string_pretty(&export)?)?;

    Ok(import_json.into())
}

// --------------------------------------------------
pub fn get_uniprot_entries(
    given_uniprot_ids: Option<Vec<String>>,
    fasta_sequence_file: &Path,
    blast_dir: &Path,
) -> Result<(Vec<UniprotEntry>, Vec<String>)> {
    let mut uniprot_ids: Vec<String> = given_uniprot_ids
        .unwrap_or_default()
        .iter()
        .map(|id| id.to_uppercase())
        .collect();
    let mut warnings = vec![];

    if uniprot_ids.is_empty() {
        // There are no given Uniprot IDs, so search
        let swissprot_ids =
            blast_uniprot(&fasta_sequence_file, &blast_dir, UniprotDb::Swissprot)?;

        if swissprot_ids.is_empty() {
            // Second-tier hits from Trembl
            let trembl_ids =
                blast_uniprot(&fasta_sequence_file, blast_dir, UniprotDb::Trembl)?;

            if let Some(first) = trembl_ids.first() {
                uniprot_ids.push(first.clone());
            }
        } else {
            // Take the best Swissprot match
            if let Some(first) = swissprot_ids.first() {
                uniprot_ids.push(first.clone());
            }
        }
    } else {
        // User-provided IDs might be duplicated
        uniprot_ids.sort();
        uniprot_ids.dedup();

        // Check if the given IDs are found in Swissprot/Trembl
        let swissprot_ids =
            blast_uniprot(&fasta_sequence_file, blast_dir, UniprotDb::Swissprot)?;

        let not_swissprot: Vec<_> = uniprot_ids
            .iter()
            .filter(|id| !swissprot_ids.contains(&id))
            .collect();

        if !not_swissprot.is_empty() {
            let trembl_ids =
                blast_uniprot(&fasta_sequence_file, blast_dir, UniprotDb::Trembl)?;

            let not_trembl: Vec<_> = not_swissprot
                .iter()
                .filter(|id| !trembl_ids.contains(&id))
                .map(|val| val.to_string())
                .collect();

            if !not_trembl.is_empty() {
                warnings.push(format!(
                    "Given Uniprot IDs not found in Swisspot or Trembl: {}",
                    not_trembl.join(", "),
                ));
            }
        }
    }

    let mut entries = vec![];
    for uniprot_id in uniprot_ids {
        match get_uniprot_entry(&uniprot_id) {
            Ok(entry) => entries.push(entry),
            Err(e) => debug!("{e}"),
        }
    }

    Ok((entries, warnings))
}

// --------------------------------------------------
pub fn check_ligand(
    ligand: &metadata::Ligand,
    inferred_ligand: &InferredLigand,
    script_dir: &Path,
) -> Result<CheckedLigand> {
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    let script = script_dir.join("compare_smiles.py");
    let mut cmd = Command::new(&uv);
    cmd.current_dir(script_dir).args([
        "run",
        script.to_string_lossy().as_ref(),
        &ligand.smiles,
        &inferred_ligand.structure.smiles,
    ]);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(str::from_utf8(&output.stderr)?.to_string());
    }

    let stdout = str::from_utf8(&output.stdout)?;
    let checked = serde_json::from_str(stdout)?;
    Ok(checked)
}

// --------------------------------------------------
pub fn get_inferred_ligands(
    min_pdb: &Path,
    min_gro: &Path,
    script_dir: &Path,
) -> Result<Vec<InferredLigand>> {
    let processed_dir = min_pdb
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", min_pdb.display()))?;
    let out_file = processed_dir.join("inferred_ligands.json");
    if file_exists(&out_file) {
        debug!("Inferred ligands file exists");
    } else {
        debug!("Creating inferred ligands file");
        let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
        let mol_tools = script_dir.join("mol_tools.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            mol_tools.to_string_lossy().as_ref(),
            "both",
            "--pdb",
            min_pdb.to_string_lossy().as_ref(),
            "--gro",
            min_gro.to_string_lossy().as_ref(),
            "--outfile",
            out_file.to_string_lossy().as_ref(),
        ]);

        debug!("Running {cmd:?}");

        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        // The script throws an exception when no ligands are found
        // But the simulation may just be in APO form, so report and move on
        if !output.status.success() {
            debug!("{}", str::from_utf8(&output.stderr)?.to_string());
        }
    }

    if file_exists(&out_file) {
        let contents = fs::read_to_string(&out_file)?;
        let ligands: Vec<InferredLigand> = serde_json::from_str(&contents)?;
        Ok(ligands)
    } else {
        Ok(vec![])
    }
}

// --------------------------------------------------
pub fn get_duration(full_xtc: &Path, integration_timestep_fs: u32) -> Result<Duration> {
    let processed_dir = full_xtc
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", full_xtc.display()))?;
    let out_file = processed_dir.join("duration.json");

    if file_exists(&out_file) {
        debug!("Duration file exists");
    } else {
        debug!("Creating duration file");
        let mut cmd = Command::new("molly");
        cmd.args(["--info", full_xtc.to_string_lossy().as_ref()]);
        debug!("Running {cmd:?}");
        let output = cmd.output()?;

        if !output.status.success() {
            bail!(str::from_utf8(&output.stderr)?.to_string());
        }

        let stdout = str::from_utf8(&output.stdout)?.to_string();
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
                    debug!("Failed to parse time_start from \"{start}\" ({line})")
                }

                if let Ok(tmp) = stop.parse::<u64>() {
                    time_stop = Some(tmp);
                } else if let Ok(tmp) = stop.parse::<f64>() {
                    time_stop = format!("{}", tmp.round()).parse::<u64>().ok();
                } else {
                    debug!("Failed to parse time_start from \"{stop}\" ({line})")
                }
            } else if let Some(caps) = MOLLY_NFRAMES_REGEX.captures(line) {
                let val = caps
                    .get(1)
                    .ok_or_else(|| anyhow!("Missing nframes value in: {line}"))?
                    .as_str();
                if let Ok(tmp) = val.parse::<u64>() {
                    num_frames = Some(tmp);
                } else {
                    debug!("Failed to parse num_frames from \"{val}\" ({line})")
                }
            }
        }
        let (time_start, time_stop, num_frames) =
            match (time_start, time_stop, num_frames) {
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
        let nstxout = sampling_ps / (integration_timestep_fs as f64 / FS_PER_PS);

        // A reasonable nstxout is 1e3..1e7. If it's way too large
        // but dividing by 1000 fixes it, the XTC timestamps are
        // inflated by 1000x (a known issue with some MD engines).
        if nstxout > 1e7 {
            let corrected_nstxout = nstxout / XTC_INFLATION_FACTOR;
            if (1e3..=1e7).contains(&corrected_nstxout) {
                debug!(
                    "XTC timestamps appear inflated by 1000x \
                         (nstxout={nstxout:.0}, corrected={corrected_nstxout:.0}). \
                         Applying correction."
                );
                duration_ps /= XTC_INFLATION_FACTOR;
            } else {
                bail!(
                    "XTC timestamps look wrong (nstxout={nstxout:.0}) \
                         but no clean 1000x correction found"
                );
            }
        }

        let totaltime_ns = (duration_ps / PS_PER_NS).round();
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
        year: *doi_paper
            .published
            .date_parts
            .first()
            .ok_or_else(|| anyhow!("DOI publication data has no year"))?,
        pages: Some(doi_paper.page.clone()),
        doi: Some(doi.to_string()),
    })
}

// --------------------------------------------------
pub fn get_uniprot_entry(uniprot_id: &str) -> Result<UniprotEntry> {
    let url = format!("https://rest.uniprot.org/uniprotkb/{uniprot_id}.json");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!(r#"Failed to fetch "{url}" ({})""#, resp.status());
    }

    let uniprot: UniprotResponse = resp.json().map_err(|e| {
        anyhow!(r#"Failed to parse Uniprot response for "{uniprot_id}": {e}"#)
    })?;

    let desc = uniprot.protein_description;
    let name = if let Some(name) = desc.recommended_name {
        name.full_name.value
    } else if let Some(name) = desc.submission_names {
        name.full_name.value
    } else {
        bail!(r#"Uniprot entry for "{uniprot_id}" has no names"#)
    };

    Ok(UniprotEntry {
        uniprot_id: uniprot_id.to_string(),
        name,
        sequence: uniprot.sequence.value,
    })
}

// --------------------------------------------------
pub fn get_pdb_entry(pdb_id: &str) -> Result<PdbEntry> {
    let pdb_id = pdb_id.to_uppercase();
    let url = format!("https://data.rcsb.org/rest/v1/core/entry/{pdb_id}");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!("Failed to GET \"{url}\" ({})", resp.status());
    }
    let pdb_resp: PdbResponse = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse PDB response: {e}"))?;

    Ok(PdbEntry {
        pdb_id: pdb_id.to_string(),
        title: pdb_resp.struct_.title.to_string(),
        classification: pdb_resp.struct_keywords.pdbx_keywords.to_string(),
    })
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
