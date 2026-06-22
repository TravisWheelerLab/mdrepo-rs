use crate::{
    types::{
        BlastResult, CheckedLigand, DoiPaper, Duration, Export, ExportSimulation,
        ImportJsonArgs, ImportResult, InferredLigand, MdFile, PdbEntry, PdbResponse,
        ProcessArgs, ProcessTrajectoryArgs, ProcessedTarball, ProcessedTrajectory,
        ProcessedTrajectoryType, PushResult, RmsdRmsf, RunImportArgs, UniprotDb,
        UniprotEntry, UniprotResponse,
    },
    validate,
};
use anyhow::{anyhow, bail, Result};
use dotenvy::dotenv;
use libmdrepo::{
    common::{file_exists, get_md5, read_file},
    constants::{MOLLY_NFRAMES_REGEX, MOLLY_TIME_REGEX},
    metadata::{self, Meta, MetaCheckOptions},
};
use log::debug;
use rayon::prelude::*;
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::{self, Path, PathBuf},
    process::Command,
    time::Instant,
};
use strum::IntoEnumIterator;
use tempfile::NamedTempFile;
use which::which;

// ── BLAST parameters ──────────────────────────────────────────────────────────
const BLAST_EVALUE: &str = "1e-5";
const BLAST_NUM_THREADS: &str = "2"; // 16 -> or make variable
const BLAST_MAX_TARGET_SEQS_SWISSPROT: &str = "20";
const BLAST_MAX_TARGET_SEQS_TREMBL: &str = "100";
const BLAST_MIN_PIDENT: f64 = 100.0;

// ── Unit conversions ──────────────────────────────────────────────────────────
const FS_PER_PS: f64 = 1000.0;
const PS_PER_NS: f64 = 1000.0;
const XTC_INFLATION_FACTOR: f64 = 1000.0;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<Vec<String>> {
    debug!("{args:?}");
    dotenv().ok();

    let start_time = Instant::now();
    let input_dir = path::absolute(&args.input_dir)?;

    // Resolve these early so canonicalization can run before validation
    let script_dir = args.script_dir.clone().unwrap_or(PathBuf::from(
        env::var("SCRIPT_DIR").map_err(|e| anyhow!("SCRIPT_DIR: {e}"))?,
    ));
    let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;
    let meta_path = input_dir.join("mdrepo-metadata.toml");

    // Canonicalize ligand SMILES before validation so non-standard notation
    // (e.g. [N+H3]) is normalised to the form the validator accepts ([NH3+])
    canonicalize_toml_smiles(&meta_path, &script_dir, &uv)?;

    // Validate
    let meta_check_opts = args.no_id.then_some(MetaCheckOptions {
        allow_no_pdb_uniprot: true,
    });

    // Check input files
    match validate::validate(&input_dir, meta_check_opts) {
        Err(e) => bail!("{e}"),
        Ok(errors) => {
            if !errors.is_empty() {
                bail!("Errors:\n{}", errors.join("\n"))
            }
        }
    }

    let processed_dir = args
        .out_dir
        .clone()
        .map_or(input_dir.join("processed"), PathBuf::from);
    let work_dir = args.work_dir.clone().unwrap_or(PathBuf::from(
        env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
    ));
    let blast_dir = work_dir.join("blast");

    debug!(r#"Processed files will go to "{processed_dir:?}""#);
    if args.force && processed_dir.is_dir() {
        debug!("Removing processed directory");
        fs::remove_dir_all(&processed_dir)?;
    }

    let meta = Meta::from_file(&meta_path)?;
    let mut trajectory_file_names = meta.trajectory_file_names.clone();
    trajectory_file_names.sort();

    // Resolve the simproc env prefix once (serially) so per-trajectory work can
    // invoke its python directly rather than via `micromamba run`.
    let simproc_prefix = find_conda_env_prefix("simproc")?;
    debug!("Using simproc env prefix: {}", simproc_prefix.display());

    let mut processed_trajectories = trajectory_file_names
        .into_par_iter()
        .enumerate()
        .map(|(trajectory_num, trajectory_file_name)| {
            process_trajectory(ProcessTrajectoryArgs {
                trajectory_num,
                trajectory_file_name: &trajectory_file_name,
                structure_file_name: &meta.structure_file_name,
                topology_file_name: &meta.topology_file_name,
                input_dir: &input_dir,
                processed_dir: &processed_dir,
                script_dir: &script_dir,
                uv: &uv,
                simproc_prefix: &simproc_prefix,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Sort by size/trajectory name
    processed_trajectories.sort_by(|a, b| {
        a.full_xtc_size
            .cmp(&b.full_xtc_size)
            .then_with(|| a.trajectory_file_stem.cmp(&b.trajectory_file_stem))
    });

    let replicates: Vec<_> = processed_trajectories
        .iter()
        .map(|val| val.trajectory_file_name.to_string())
        .collect();

    let mut errors: Vec<String> = processed_trajectories
        .iter()
        .flat_map(|t| t.errors.iter().cloned())
        .collect();

    // The processed trajectories are returned sorted by full xtc size and filename
    let example_trajectory = processed_trajectories
        .last()
        .ok_or_else(|| anyhow!("Unable to get last of processed trajectories"))?;

    let trajectory_tarballs =
        make_trajectory_tarballs(&processed_dir, &processed_trajectories)?;

    let import_json = &processed_dir.join("import.json");
    let import_warnings = make_import_json(ImportJsonArgs {
        meta,
        import_json: &import_json,
        processed_dir: &processed_dir,
        meta_path: &meta_path,
        input_dir: &input_dir,
        script_dir: &script_dir,
        blast_dir: &blast_dir,
        uv: &uv,
        example_trajectory,
        all_trajectories: &processed_trajectories,
        trajectory_tarballs: &trajectory_tarballs,
        reprocess_simulation_id: args.reprocess_simulation_id,
        replicates: &replicates,
        replace_original_files: args.replace_original_files,
    })?;
    errors.extend(import_warnings);

    if !args.dry_run {
        let import_result = run_import(RunImportArgs {
            uv: &uv,
            script_dir: &script_dir,
            import_json: &import_json,
            input_dir: &input_dir,
            server: &args.server.to_string(),
            reprocess_simulation_id: args.reprocess_simulation_id,
            processed_dir: &processed_dir,
            replace_original_files: args.replace_original_files,
        })?;

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

    debug!("Finished processing in {:?}", start_time.elapsed());

    let errors_file = input_dir.join("processing_errors.txt");
    if errors.is_empty() {
        if errors_file.exists() {
            fs::remove_file(&errors_file)?;
        }
        debug!("No errors");
    } else {
        let errors_fh = File::create(&errors_file)?;
        write!(&errors_fh, "{}", errors.join("\n"))?;
        let num_errors = errors.len();
        debug!(
            r#"Wrote {num_errors} error{} to "{}""#,
            if num_errors == 1 { "" } else { "s" },
            errors_file.display()
        );
    }

    Ok(errors)
}

// --------------------------------------------------
fn run_import(args: RunImportArgs) -> Result<ImportResult> {
    let import_script = args.script_dir.join("import_preprocessed.py");
    let out_file = args.processed_dir.join("imported.json");
    debug!(r#"Import "{}""#, args.import_json.display());

    let mut cmd_args = vec![
        "run".to_string(),
        import_script.to_string_lossy().to_string(),
        "--file".to_string(),
        args.import_json.to_string_lossy().to_string(),
        "--data-dir".to_string(),
        args.input_dir.to_string_lossy().to_string(),
        "--server".to_string(),
        args.server.to_string(),
        "--out-file".to_string(),
        out_file.to_string_lossy().to_string(),
    ];

    if let Some(id) = args.reprocess_simulation_id {
        cmd_args.extend(["--simulation-id".to_string(), id.to_string()]);
    }

    if args.replace_original_files {
        cmd_args.extend(["--replace-original-files".to_string()]);
    }

    let mut cmd = Command::new(args.uv);
    cmd.current_dir(args.script_dir).args(&cmd_args);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr));
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
    reprocess_simulation_id: Option<u64>,
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
    }

    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir).args(&args);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !file_exists(&out_file) {
        let stdout = str::from_utf8(&output.stdout)?;
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
    uv: &Path,
) -> Result<()> {
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
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !file_exists(thumbnail) {
            bail!(r#"Failed to create "{}""#, thumbnail.display());
        }
    }
    Ok(())
}

// --------------------------------------------------
/// Discover the filesystem prefix of a named conda/micromamba environment.
///
/// We resolve this once, serially, at the start of processing so that the
/// per-trajectory work can invoke the environment's interpreter directly
/// instead of wrapping every call in `micromamba run`. `micromamba run`
/// registers each process in a lock-guarded registry
/// (`$MAMBA_ROOT_PREFIX/proc`); under heavy parallelism dozens of concurrent
/// invocations contend on that single lock and intermittently fail with
/// "Could not set lock (Resource temporarily unavailable)". Activating the
/// env ourselves (PATH + AMBERHOME) sidesteps the registry entirely.
fn find_conda_env_prefix(env_name: &str) -> Result<PathBuf> {
    let micromamba =
        which("micromamba").map_err(|e| anyhow!("Failed to find micromamba ({e})"))?;
    let output = Command::new(&micromamba)
        .args(["env", "list", "--json"])
        .output()?;
    if !output.status.success() {
        bail!(
            "micromamba env list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let envs = parsed
        .get("envs")
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("Unexpected output from `micromamba env list --json`"))?;
    for env in envs {
        if let Some(path) = env.as_str() {
            let prefix = PathBuf::from(path);
            if prefix.file_name().and_then(|n| n.to_str()) == Some(env_name)
                && prefix.join("bin").is_dir()
            {
                return Ok(prefix);
            }
        }
    }
    bail!("Could not find conda env '{env_name}' via `micromamba env list`");
}

// --------------------------------------------------
pub fn process_trajectory(args: ProcessTrajectoryArgs) -> Result<ProcessedTrajectory> {
    let mut errors = vec![];
    let trajectory_dir = args
        .processed_dir
        .join(format!("rep_{}", args.trajectory_num + 1));

    if !trajectory_dir.is_dir() {
        fs::create_dir_all(&trajectory_dir)?;
    }

    let full_min_files = &[
        "full.gro",
        "full.pdb",
        "full.xtc",
        "minimal.gro",
        "minimal.pdb",
        "minimal.xtc",
    ]
    .map(|filename| trajectory_dir.join(filename));

    if full_min_files.iter().all(|f| file_exists(f)) {
        debug!(
            r#"Full/minimal files all exist for "{}""#,
            args.trajectory_file_name
        );
    } else {
        let mut trajectory_path = args.input_dir.join(args.trajectory_file_name);
        let trajectory_ext = trajectory_path.extension().ok_or_else(|| {
            anyhow!(
                r#"Failed to get file extension for "{}""#,
                args.trajectory_file_name
            )
        })?;

        if trajectory_ext == "mdc" {
            let mdcompress = which("mdcompress")
                .map_err(|e| anyhow!("Failed to find mdcompress ({e})"))?;
            let trajectory_stem = trajectory_path.file_stem().ok_or_else(|| {
                anyhow!(
                    r#"Failed to get file stem for "{}""#,
                    args.trajectory_file_name
                )
            })?;
            let xtc_path = args
                .input_dir
                .join(format!("{}.xtc", trajectory_stem.to_string_lossy()));
            let mut cmd = Command::new(mdcompress);
            cmd.arg("decompress")
                .arg("-i")
                .arg(&trajectory_path)
                .arg("-o")
                .arg(&xtc_path);

            debug!("Running {cmd:?}");

            let output = cmd.output()?;
            if !output.status.success() {
                bail!("{}", String::from_utf8_lossy(&output.stderr));
            }

            if !file_exists(&xtc_path) {
                bail!(
                    r#"Failed to decompress {} to {}"#,
                    trajectory_path.to_string_lossy(),
                    xtc_path.to_string_lossy(),
                );
            }
            trajectory_path = xtc_path;
        }

        debug!("Making full/minimal files");

        let cpp_traj = args.script_dir.join("cpptraj_gmx_traj_manipulation.py");
        if !cpp_traj.is_file() {
            bail!(r#"Missing "{}""#, cpp_traj.display());
        }

        // Invoke the simproc env's python directly rather than via
        // `micromamba run`. The latter registers each process under a single
        // lock-guarded registry, which intermittently fails under the heavy
        // parallelism here. We reproduce the parts of env activation the
        // script actually needs: its bin/ on PATH (cpptraj) and AMBERHOME.
        let simproc_bin = args.simproc_prefix.join("bin");
        let python = simproc_bin.join("python");
        let path_var = env::var_os("PATH").unwrap_or_default();
        let new_path = env::join_paths(
            std::iter::once(simproc_bin).chain(env::split_paths(&path_var)),
        )?;

        let coord = args.input_dir.join(args.structure_file_name);
        let top = args.input_dir.join(args.topology_file_name);
        let script_args = [
            cpp_traj.to_string_lossy().to_string(),
            "--traj".into(),
            trajectory_path.to_string_lossy().into(),
            "--coord".into(),
            coord.to_string_lossy().into(),
            "--top".into(),
            top.to_string_lossy().into(),
            "--outdir".into(),
            trajectory_dir.to_string_lossy().into(),
        ];

        let mut cmd = Command::new(python);
        cmd.env("PATH", new_path)
            .env("AMBERHOME", args.simproc_prefix)
            .args(&script_args);

        // We run python directly (above) to dodge micromamba's global lock, but
        // log the equivalent `micromamba run` form: copy/paste it to reproduce a
        // single replicate by hand (one manual run never contends on the lock).
        debug!(
            "Running (reproduce with: micromamba run -n simproc python {})",
            script_args
                .iter()
                .map(|a| format!("{a:?}"))
                .collect::<Vec<_>>()
                .join(" ")
        );

        let output = cmd.output()?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }

        let missing: Vec<_> = full_min_files
            .iter()
            .filter(|f| !file_exists(f))
            .map(|f| f.to_string_lossy().to_string())
            .collect();

        if !missing.is_empty() {
            let error_file = &trajectory_dir.join("errors.txt");
            let error_fh = File::create(&error_file)?;
            write!(
                &error_fh,
                "Failed command {cmd:?}\nFailed to create {}\n{}\n{}",
                missing.join(", "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )?;
            errors.push(error_file.to_string_lossy().to_string());
        }
    }

    let full_gro = trajectory_dir.join("full.gro");
    let full_pdb = trajectory_dir.join("full.pdb");
    let full_xtc = trajectory_dir.join("full.xtc");
    let min_gro = trajectory_dir.join("minimal.gro");
    let min_pdb = trajectory_dir.join("minimal.pdb");
    let min_xtc = trajectory_dir.join("minimal.xtc");
    let sampled_xtc = trajectory_dir.join("sampled.xtc");
    let thumbnail_png = trajectory_dir.join("thumbnail.png");
    let full_xtc_size = fs::metadata(&full_xtc)?.len();

    println!("check_coarse_grained");
    let is_coarse_grained = check_coarse_grained(&full_pdb, &min_pdb)?;

    println!("sample_trajectory");
    sample_trajectory(&min_xtc, &min_pdb, &sampled_xtc, args.script_dir, args.uv)?;
    println!("make_thumbnail");
    make_thumbnail(
        &thumbnail_png,
        &sampled_xtc,
        &min_pdb,
        args.script_dir,
        args.uv,
    )?;

    let trajectory_file_stem = Path::new(&args.trajectory_file_name)
        .file_stem()
        .ok_or_else(|| {
            anyhow!(
                r#"Failed to extract file_stem from "{}""#,
                args.trajectory_file_name
            )
        })?
        .to_string_lossy()
        .to_string();

    Ok(ProcessedTrajectory {
        full_gro,
        full_pdb,
        full_xtc,
        min_gro,
        min_pdb,
        min_xtc,
        sampled_xtc,
        thumbnail_png,
        full_xtc_size,
        trajectory_file_name: args.trajectory_file_name.to_string(),
        trajectory_file_stem,
        directory_name: trajectory_dir.to_string_lossy().to_string(),
        is_coarse_grained,
        errors,
    })
}

// --------------------------------------------------
pub fn check_coarse_grained(full_pdb: &Path, min_pdb: &Path) -> Result<bool> {
    // TEMPORARY FIX TO REMOVE REMARK UNTIL PR IS ACCEPTED
    let mut tmp = NamedTempFile::new()?;
    for line in BufReader::new(File::open(min_pdb)?).lines() {
        let line = line?;
        if !line.starts_with("REMARK") {
            writeln!(tmp, "{line}")?;
        }
    }

    let structure = pdbrust::parse_pdb_file(tmp.path())
        .map_err(|err| anyhow!("{}: {err}", min_pdb.display()))?;
    let protein = structure.select("protein")?;
    let total_atoms = protein.get_num_atoms();
    let unique_residues: HashSet<_> = protein
        .atoms
        .iter()
        .map(|a| (a.chain_id.clone(), a.residue_seq, a.ins_code))
        .collect();
    let num_residues = unique_residues.len();
    let is_coarse_grained = total_atoms == num_residues;
    debug!(
        "Checking if coarse-grained ({})",
        if is_coarse_grained { "Yes" } else { "No" }
    );

    if is_coarse_grained {
        for path in [min_pdb, full_pdb] {
            fix_alpha_carbon_name(path)?;
        }
    }

    Ok(is_coarse_grained)
}

// --------------------------------------------------
fn fix_alpha_carbon_name(path: &Path) -> Result<()> {
    // TEMPORARY FIX TO REMOVE REMARK UNTIL PR IS ACCEPTED
    let mut tmp = NamedTempFile::new()?;
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        if !line.starts_with("REMARK") {
            writeln!(tmp, "{line}")?;
        }
    }
    let structure = pdbrust::parse_pdb_file(tmp.path())?;
    let protein = structure.select("protein")?;
    let total_atoms = protein.get_num_atoms();
    let protein_serials: HashSet<i32> = protein
        .atoms
        .iter()
        .filter_map(|a| (a.name == "A").then_some(a.serial))
        .collect();

    if protein_serials.len() == total_atoms {
        debug!(
            r#"All proteins in "{}" are named "A," changing to "CA""#,
            path.display()
        );
        let mut fixed = structure.clone();
        for atom in fixed.atoms.iter_mut() {
            if protein_serials.contains(&atom.serial) {
                atom.name = "CA".to_string();
            }
        }

        // Backup original and then overwrite
        let stem = path.file_stem().unwrap_or_default();
        let backup =
            path.with_file_name(format!("{}_orig.pdb", stem.to_string_lossy()));
        fs::rename(path, backup)?;
        fixed.to_file(&path)?;
    }

    Ok(())
}

// --------------------------------------------------
pub fn get_rmsd_rmsf(
    min_pdb: &Path,
    min_xtc: &Path,
    processed_dir: &Path,
    script_dir: &Path,
    uv: &Path,
) -> Result<RmsdRmsf> {
    let out_file = processed_dir.join("rmsd_rmsf.json");

    if file_exists(&out_file) {
        debug!("RMSD/RMSF file exists");
    } else {
        debug!("Creating RMSD/RMSF file");
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
            bail!("{}", String::from_utf8_lossy(&output.stderr));
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

    let processed_dir = fasta_sequence.parent().ok_or_else(|| {
        anyhow!("No parent directory for '{}'", fasta_sequence.display())
    })?;
    let blast_results = processed_dir.join(format!(
        "blast.{}.tsv",
        uniprot_db.to_string().to_lowercase()
    ));

    if file_exists(&blast_results) {
        debug!(
            "{uniprot_db} BLAST results exists ({})",
            blast_results.display()
        );
    } else {
        debug!(
            "Creating {uniprot_db} BLAST results ({})",
            blast_results.display()
        );
        let blastp =
            which("blastp").map_err(|e| anyhow!("Failed to find blastp ({e})"))?;

        let mut cmd = Command::new(&blastp);
        let (blast_db, max_target_seqs) = match uniprot_db {
            UniprotDb::Swissprot => (
                blast_dir.join("swissprot").join("swissprot"),
                BLAST_MAX_TARGET_SEQS_SWISSPROT,
            ),
            UniprotDb::Isoform => (
                blast_dir.join("isoform").join("isoform"),
                BLAST_MAX_TARGET_SEQS_SWISSPROT,
            ),
            UniprotDb::Trembl => (
                blast_dir.join("trembl").join("trembl"),
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
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }
    }

    let mut results = vec![];
    if file_exists(&blast_results) {
        let file = BufReader::new(
            File::open(&blast_results)
                .map_err(|e| anyhow!("{}: {e}", blast_results.display()))?,
        );

        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .has_headers(false)
            .from_reader(file);

        // Swissprot also covers Isoform
        let swissprot_regex = Regex::new(r"^sp[|]([^|]+)[|]")?;
        let trembl_regex = Regex::new(r"^tr[|]([^|]+)[|]")?;
        for result in reader.deserialize() {
            let hit: BlastResult =
                result.map_err(|e| anyhow!("{}: {e}", blast_results.display()))?;
            if hit.pident >= BLAST_MIN_PIDENT {
                let hit_name =
                    if let Some(caps) = swissprot_regex.captures(&hit.saccver) {
                        caps.get(1)
                            .ok_or_else(|| anyhow!("regex capture group 1 not found"))?
                            .as_str()
                            .to_string()
                    } else if let Some(caps) = trembl_regex.captures(&hit.saccver) {
                        caps.get(1)
                            .ok_or_else(|| anyhow!("regex capture group 1 not found"))?
                            .as_str()
                            .to_string()
                    } else {
                        hit.saccver
                    };

                results.push(hit_name)
            }
        }
    }

    Ok(results)
}

// --------------------------------------------------
pub fn get_sequence(
    full_pdb: &Path,
    processed_dir: &Path,
    script_dir: &Path,
    uv: &Path,
) -> Result<PathBuf> {
    let sequence_file = processed_dir.join("sequence.fa");

    if file_exists(&sequence_file) {
        debug!("Sequence file exists");
    } else {
        debug!("Creating sequence file");
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
            bail!("{}", String::from_utf8_lossy(&output.stderr));
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
    uv: &Path,
) -> Result<()> {
    if file_exists(out_file) {
        debug!("Sampled trajectory exists");
    } else {
        debug!("Creating sampled trajectory");
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
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }

        if !file_exists(out_file) {
            bail!(r#"Failed to create "{}""#, out_file.display());
        }
    }

    Ok(())
}

// --------------------------------------------------
pub fn make_trajectory_tarballs(
    processed_dir: &Path,
    processed_trajectories: &[ProcessedTrajectory],
) -> Result<Vec<ProcessedTarball>> {
    let mut tarballs = vec![];
    if processed_trajectories.len() > 1 {
        for trajectory_type in ProcessedTrajectoryType::iter() {
            let trajectory_type_lc = trajectory_type.to_string().to_lowercase();
            let tarball_name = format!("{trajectory_type_lc}.tar");
            let tarball_path = processed_dir.join(&tarball_name);
            if tarball_path.is_file() {
                fs::remove_file(&tarball_path)?;
            }

            let tar_dir = processed_dir.join(&trajectory_type_lc);
            if tar_dir.is_dir() {
                fs::remove_dir_all(&tar_dir)?;
            }
            fs::create_dir_all(&tar_dir)?;

            for processed in processed_trajectories {
                let trajectory = match trajectory_type {
                    ProcessedTrajectoryType::Full => processed.full_xtc.clone(),
                    ProcessedTrajectoryType::Minimal => processed.min_xtc.clone(),
                    ProcessedTrajectoryType::Sampled => processed.sampled_xtc.clone(),
                };
                std::os::unix::fs::symlink(
                    trajectory,
                    tar_dir.join(format!("{}.xtc", processed.trajectory_file_stem)),
                )?;
            }

            let mut cmd = Command::new("tar");
            cmd.current_dir(processed_dir).args([
                "--dereference", // follow symlinks
                "-cf",           // create file
                &tarball_name,
                &trajectory_type_lc,
            ]);

            debug!("Running {cmd:?}");

            let output = cmd.output()?;
            if !output.status.success() {
                bail!("{}", String::from_utf8_lossy(&output.stderr));
            }
            fs::remove_dir_all(&tar_dir)?;

            tarballs.push(ProcessedTarball {
                path: tarball_path,
                file_type: format!("{trajectory_type} Trajectories (All)"),
            });
        }
    }

    Ok(tarballs)
}

// --------------------------------------------------
pub fn make_import_json(args: ImportJsonArgs) -> Result<Vec<String>> {
    let structure_hash =
        get_file_hash(&args.input_dir.join(&args.meta.structure_file_name))?;

    let fasta_sequence_file = get_sequence(
        &args.example_trajectory.full_pdb,
        args.processed_dir,
        args.script_dir,
        args.uv,
    )?;

    let rmsd_rmsf = get_rmsd_rmsf(
        &args.example_trajectory.min_pdb,
        &args.example_trajectory.min_xtc,
        args.processed_dir,
        args.script_dir,
        args.uv,
    )?;

    let duration = get_duration(
        args.all_trajectories,
        &args.example_trajectory.full_xtc,
        args.meta.integration_timestep_fs,
        args.processed_dir,
    )?;

    let inferred_ligands = get_inferred_ligands(
        &args.example_trajectory.min_pdb,
        args.processed_dir,
        args.script_dir,
        args.uv,
    )?;

    let unique_file_hash_string = get_unique_file_hash(&args.meta, args.input_dir);

    let (uniprots, uniprot_warnings) = get_uniprot_entries(
        args.meta.uniprot_ids.clone(),
        &fasta_sequence_file,
        args.blast_dir,
    )?;

    let (ligands, ligand_warnings) = resolve_ligands(
        args.meta.ligands.as_ref(),
        inferred_ligands,
        args.script_dir,
        args.uv,
    )?;

    let mut warnings = uniprot_warnings;
    warnings.extend(ligand_warnings);

    let mut pdb = None;
    if let Some(pdb_id) = &args.meta.pdb_id {
        match get_pdb_entry(pdb_id) {
            Ok(entry) => pdb = Some(entry),
            Err(e) => warnings.push(e.to_string()),
        }
    }

    let original_files =
        if args.reprocess_simulation_id.is_none() || args.replace_original_files {
            collect_original_files(
                &args.meta,
                args.meta_path,
                args.input_dir,
                args.example_trajectory,
            )?
        } else {
            vec![]
        };

    let processed_files = collect_processed_files(
        args.processed_dir,
        args.example_trajectory,
        args.trajectory_tarballs,
    )?;

    let mut papers: Vec<metadata::Paper> = args.meta.papers.unwrap_or_default();
    if let Some(dois) = &args.meta.dois {
        for doi in dois {
            match get_doi(doi) {
                Ok(paper) => papers.push(paper),
                Err(e) => debug!("{e}"),
            }
        }
    }

    let (water_type, water_density) = match args.meta.water {
        Some(water) => (Some(water.model), Some(water.density_kg_m3)),
        _ => (None, None),
    };

    let fasta_sequence = fs::read_to_string(fasta_sequence_file)?;
    let simulation = ExportSimulation {
        simulation_id: args.reprocess_simulation_id,
        lead_contributor_orcid: args.meta.lead_contributor_orcid,
        unique_file_hash_string,
        alias: args.meta.alias,
        description: args.meta.description,
        short_description: args.meta.short_description,
        run_commands: args.meta.run_commands,
        software_name: args.meta.software_name,
        software_version: args.meta.software_version,
        pdb,
        uniprots,
        duration: duration.totaltime_ns,
        sampling_frequency: duration.sampling_frequency_ns,
        integration_timestep_fs: args.meta.integration_timestep_fs,
        external_links: args.meta.external_links.unwrap_or_default(),
        forcefield: args.meta.forcefield,
        forcefield_comments: args.meta.forcefield_comments,
        protonation_method: args.meta.protonation_method,
        rmsd_values: rmsd_rmsf.rmsd,
        rmsf_values: rmsd_rmsf.rmsf,
        temperature_kelvin: args.meta.temperature_kelvin,
        fasta_sequence,
        num_replicates: args.meta.trajectory_file_names.len() as u32,
        water_type,
        water_density,
        structure_hash,
        contributors: args.meta.contributors.unwrap_or_default(),
        original_files,
        processed_files,
        ligands,
        solutes: args.meta.solutes.unwrap_or_default(),
        papers,
        is_embargoed: args.meta.is_embargoed,
        is_coarse_grained: Some(args.example_trajectory.is_coarse_grained),
        replicates: args.replicates.to_vec(),
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
        warnings: warnings.clone(),
    };

    debug!(r#"Writing JSON to "{}""#, args.import_json.display());
    let file = File::create(args.import_json)?;
    writeln!(&file, "{}", &serde_json::to_string_pretty(&export)?)?;

    Ok(warnings)
}

// --------------------------------------------------
fn resolve_ligands(
    given_ligands: Option<&Vec<metadata::Ligand>>,
    inferred_ligands: Vec<InferredLigand>,
    script_dir: &Path,
    uv: &Path,
) -> Result<(Vec<metadata::Ligand>, Vec<String>)> {
    let mut ligands = vec![];
    let mut warnings = vec![];

    if let Some(given_ligands) = given_ligands {
        ligands = given_ligands.clone();

        if !inferred_ligands.is_empty() {
            for (ligand_num, given_ligand) in given_ligands.iter().enumerate() {
                let mut found_match = false;
                for inferred in &inferred_ligands {
                    let check = check_ligand(given_ligand, inferred, script_dir, uv)?;
                    if check.exact_match
                        || check.same_connectivity
                        || check.same_connectivity_and_stereo
                        || check.same_inchi
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
                .best_name
                .unwrap_or(ligand.name.iupac_name.unwrap_or("NA".to_string()));
            ligands.push(metadata::Ligand {
                name,
                smiles: ligand.structure.smiles,
            });
        }
    }

    Ok((ligands, warnings))
}

// --------------------------------------------------
fn collect_original_files(
    meta: &Meta,
    meta_path: &Path,
    input_dir: &Path,
    example_trajectory: &ProcessedTrajectory,
) -> Result<Vec<MdFile>> {
    let mut files = vec![];

    files.push(MdFile {
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
        ("Structure", &meta.structure_file_name),
        ("Topology", &meta.topology_file_name),
    ] {
        let local_path = input_dir.join(filename);
        files.push(MdFile {
            name: filename.to_string(),
            file_type: file_type.to_string(),
            size: local_path.metadata()?.len(),
            md5_sum: get_md5(&local_path)?,
            description: None,
            is_primary: Some(true),
        });
    }

    for filename in &meta.trajectory_file_names {
        if *filename == example_trajectory.trajectory_file_name {
            let local_path = input_dir.join(filename);
            files.push(MdFile {
                name: filename.to_string(),
                file_type: "Trajectory".to_string(),
                size: local_path.metadata()?.len(),
                md5_sum: get_md5(&local_path)?,
                description: None,
                is_primary: Some(true),
            });
        }
    }

    if meta.trajectory_file_names.len() > 1 {
        let tar_name = "trajectories.tar";
        let local_path = input_dir.join(tar_name);
        if local_path.is_file() {
            fs::remove_file(&local_path)?;
        }
        for (i, filename) in meta.trajectory_file_names.iter().enumerate() {
            let mut cmd = Command::new("tar");
            let flag = if i == 0 { "-cf" } else { "-rf" };
            cmd.current_dir(input_dir).args([flag, tar_name, filename]);
            debug!("Running {cmd:?}");
            let output = cmd.output()?;
            if !output.status.success() {
                bail!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
        files.push(MdFile {
            name: tar_name.to_string(),
            file_type: "Trajectories (All)".to_string(),
            size: local_path.metadata()?.len(),
            md5_sum: get_md5(&local_path)?,
            description: None,
            is_primary: None,
        });
    }

    if let Some(addl_files) = &meta.additional_files {
        for file in addl_files {
            let path = input_dir.join(&file.file_name);
            let md5_sum = get_md5(&path)?;
            if !files.iter().any(|f| f.md5_sum == md5_sum) {
                files.push(MdFile {
                    name: file.file_name.to_string(),
                    file_type: file.file_type.to_string(),
                    size: path.metadata()?.len(),
                    md5_sum,
                    description: file.description.clone(),
                    is_primary: None,
                });
            }
        }
    }

    Ok(files)
}

// --------------------------------------------------
fn collect_processed_files(
    processed_dir: &Path,
    example_trajectory: &ProcessedTrajectory,
    trajectory_tarballs: &[ProcessedTarball],
) -> Result<Vec<MdFile>> {
    let mut files = vec![];

    for (file_type, path) in &[
        ("Processed topology", &example_trajectory.full_gro),
        ("Processed structure", &example_trajectory.full_pdb),
        ("Processed trajectory", &example_trajectory.full_xtc),
        ("Minimal topology", &example_trajectory.min_gro),
        ("Minimal structure", &example_trajectory.min_pdb),
        ("Minimal trajectory", &example_trajectory.min_xtc),
        (
            "Sampled minimal trajectory",
            &example_trajectory.sampled_xtc,
        ),
        ("Preview image", &example_trajectory.thumbnail_png),
    ] {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow!("No filename for '{}'", path.display()))?
            .to_string_lossy()
            .to_string();

        let symlink = processed_dir.join(&filename);
        if symlink.exists() {
            fs::remove_file(&symlink)?;
        }
        std::os::unix::fs::symlink(path, &symlink)?;

        files.push(MdFile {
            name: filename,
            file_type: file_type.to_string(),
            size: path.metadata()?.len(),
            md5_sum: get_md5(path)?,
            description: None,
            is_primary: None,
        });
    }

    for tarball in trajectory_tarballs {
        files.push(MdFile {
            name: tarball
                .path
                .file_name()
                .ok_or_else(|| anyhow!("No filename for '{}'", tarball.path.display()))?
                .to_string_lossy()
                .to_string(),
            file_type: tarball.file_type.clone(),
            size: tarball.path.metadata()?.len(),
            md5_sum: get_md5(&tarball.path)?,
            description: None,
            is_primary: None,
        });
    }

    Ok(files)
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
            blast_uniprot(fasta_sequence_file, blast_dir, UniprotDb::Swissprot)?;

        if swissprot_ids.is_empty() {
            // Second-tier hits from Trembl
            let trembl_ids =
                blast_uniprot(fasta_sequence_file, blast_dir, UniprotDb::Trembl)?;

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
            blast_uniprot(fasta_sequence_file, blast_dir, UniprotDb::Swissprot)?;

        let not_in_swissprot: Vec<_> = uniprot_ids
            .iter()
            .filter(|id| !swissprot_ids.contains(id))
            .collect();

        if !not_in_swissprot.is_empty() {
            let isoform_ids =
                blast_uniprot(fasta_sequence_file, blast_dir, UniprotDb::Isoform)?;

            let mut found_in_isoform: Vec<(String, String)> = vec![];
            for uniprot_id in &not_in_swissprot {
                for isoform_id in &isoform_ids {
                    if isoform_id.starts_with(*uniprot_id) {
                        found_in_isoform
                            .push((uniprot_id.to_string(), isoform_id.clone()));
                    }
                }
            }

            for (uniprot_id, isoform_id) in &found_in_isoform {
                warnings.push(format!(
                    r#"Uniprot ID "{uniprot_id}" found in Isoform as "{isoform_id}""#,
                ));
            }

            // If we still haven't found all the Uniprot IDs, look in Trembl
            if swissprot_ids.len() + found_in_isoform.len() < uniprot_ids.len() {
                let trembl_ids =
                    blast_uniprot(fasta_sequence_file, blast_dir, UniprotDb::Trembl)?;

                let not_in_trembl: Vec<_> = not_in_swissprot
                    .iter()
                    .filter(|id| !trembl_ids.contains(id))
                    .map(|val| val.to_string())
                    .collect();

                if !not_in_trembl.is_empty() {
                    warnings.push(format!(
                        "Uniprot IDs not found in Swissprot or Trembl: {}",
                        not_in_trembl.join(", "),
                    ));
                }
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
fn canonicalize_toml_smiles(meta_path: &Path, script_dir: &Path, uv: &Path) -> Result<()> {
    let script = script_dir.join("canonicalize_toml_smiles.py");
    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir).args([
        "run",
        script.to_string_lossy().as_ref(),
        meta_path.to_string_lossy().as_ref(),
    ]);
    debug!("Canonicalizing SMILES in {}", meta_path.display());
    let output = cmd.output()?;
    if !output.status.success() {
        bail!(
            "canonicalize_toml_smiles failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

// --------------------------------------------------
pub fn check_ligand(
    ligand: &metadata::Ligand,
    inferred_ligand: &InferredLigand,
    script_dir: &Path,
    uv: &Path,
) -> Result<CheckedLigand> {
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
        bail!("{}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = str::from_utf8(&output.stdout)?;
    let checked = serde_json::from_str(stdout)?;
    Ok(checked)
}

// --------------------------------------------------
pub fn get_inferred_ligands(
    min_pdb: &Path,
    processed_dir: &Path,
    script_dir: &Path,
    uv: &Path,
) -> Result<Vec<InferredLigand>> {
    let out_file = processed_dir.join("inferred_ligands.json");
    if file_exists(&out_file) {
        debug!("Inferred ligands file exists");
    } else {
        debug!("Creating inferred ligands file");
        let mol_tools = script_dir.join("mol_id.py");
        let mut cmd = Command::new(&uv);
        cmd.current_dir(script_dir).args([
            "run",
            mol_tools.to_string_lossy().as_ref(),
            "both",
            min_pdb.to_string_lossy().as_ref(),
            "--outfile",
            out_file.to_string_lossy().as_ref(),
        ]);

        debug!("Running {cmd:?}");

        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        // The script throws an exception when no ligands are found
        // But the simulation may just be in APO form, so report and move on
        if !output.status.success() {
            debug!("{}", str::from_utf8(&output.stderr)?);
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
pub fn get_duration(
    trajectories: &[ProcessedTrajectory],
    example_full_xtc: &Path,
    integration_timestep_fs: u32,
    processed_dir: &Path,
) -> Result<Duration> {
    let out_file = processed_dir.join("duration.json");

    if file_exists(&out_file) {
        debug!("Duration file exists");
    } else {
        debug!("Creating duration file");

        // totaltime_ns is the SUM of every trajectory's duration in the bundle:
        // each entry is a set of dissociation replicas of differing length, so
        // the total simulated time is the sum, not just the representative
        // trajectory. sampling_frequency is a per-frame spacing (not additive),
        // so it is taken from the representative (example) trajectory.
        let mut total_duration_ps = 0.0;
        let mut sampling_frequency_ns = 0.0_f32;
        for traj in trajectories {
            let (duration_ps, sampling_ns) =
                measure_trajectory(&traj.full_xtc, integration_timestep_fs)?;
            total_duration_ps += duration_ps;
            if traj.full_xtc == example_full_xtc {
                sampling_frequency_ns = sampling_ns;
            }
        }

        // Keep as f64: these dissociation trajectories are sub-nanosecond, so
        // truncating to an integer would floor the duration to 0. The DB column
        // (duration) is Nullable<Float8>, so a float flows through unchanged.
        let totaltime_ns = round_dp(total_duration_ps / PS_PER_NS, 2);
        let duration = Duration {
            totaltime_ns,
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

/// Measure one trajectory with `molly --info`, returning
/// (duration_ps, sampling_frequency_ns). Includes the 1000x inflated-timestamp
/// correction used historically.
fn measure_trajectory(
    full_xtc: &Path,
    integration_timestep_fs: u32,
) -> Result<(f64, f32)> {
    let mut cmd = Command::new("molly");
    cmd.args(["--info", full_xtc.to_string_lossy().as_ref()]);
    debug!("Running {cmd:?}");
    let output = cmd.output()?;

    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = str::from_utf8(&output.stdout)?.to_string();
    let mut time_start: Option<u64> = None;
    let mut time_stop: Option<u64> = None;
    let mut num_frames: Option<u64> = None;
    for line in stdout.lines() {
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
                debug!(r#"Failed to parse time_start from "{start}" ({line})"#)
            }

            if let Ok(tmp) = stop.parse::<u64>() {
                time_stop = Some(tmp);
            } else if let Ok(tmp) = stop.parse::<f64>() {
                time_stop = format!("{}", tmp.round()).parse::<u64>().ok();
            } else {
                debug!(r#"Failed to parse time_start from "{stop}" ({line})"#)
            }
        } else if let Some(caps) = MOLLY_NFRAMES_REGEX.captures(line) {
            let val = caps
                .get(1)
                .ok_or_else(|| anyhow!("Missing nframes value in: {line}"))?
                .as_str();
            if let Ok(tmp) = val.parse::<u64>() {
                num_frames = Some(tmp);
            } else {
                debug!(r#"Failed to parse num_frames from "{val}" ({line})"#)
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

    let sampling_frequency_ns =
        round_dp((duration_ps / PS_PER_NS) / (num_frames - 1.), 3) as f32;

    Ok((duration_ps, sampling_frequency_ns))
}

// --------------------------------------------------
fn round_dp(x: f64, dp: u32) -> f64 {
    let factor = 10f64.powi(dp as i32);
    (x * factor).round() / factor
}

// --------------------------------------------------
pub fn get_file_hash(path: &Path) -> Result<String> {
    let contents = fs::read(path)?;
    let digest = Sha1::digest(&contents);
    Ok(format!("{digest:?}"))
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
        .map_err(|e| anyhow!("Failed to parse DOI response ({url}): {e}"))?;

    let authors = doi_paper
        .author
        .into_iter()
        .map(|author| format!("{} {}", author.given, author.family))
        .collect::<Vec<String>>()
        .join(", ");

    let year = if let Some(published) = doi_paper.published {
        published.date_parts.first().copied().unwrap_or(0)
    } else if let Some(issued) = doi_paper.issued {
        issued
            .date_parts
            .first()
            .and_then(|parts| parts.first().copied())
            .unwrap_or(0)
    } else {
        0
    };
    let volume = doi_paper.volume.unwrap_or(0);

    let paper = if let Some(publisher) = doi_paper.publisher {
        metadata::Paper {
            title: doi_paper.title,
            authors,
            journal: publisher,
            volume,
            number: None,
            year,
            pages: doi_paper.page,
            doi: Some(doi.to_string()),
        }
    } else {
        metadata::Paper {
            title: doi_paper.title,
            authors,
            journal: doi_paper.journal.unwrap_or("NA".to_string()),
            volume,
            number: None,
            year,
            pages: doi_paper.page,
            doi: Some(doi.to_string()),
        }
    };

    Ok(paper)
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
        pdb_id,
        title: pdb_resp.struct_.title,
        classification: pdb_resp.struct_keywords.pdbx_keywords,
    })
}

// --------------------------------------------------
pub fn get_unique_file_hash(meta: &Meta, input_dir: &Path) -> String {
    let mut input_files = vec![
        meta.structure_file_name.to_string(),
        meta.topology_file_name.to_string(),
    ];

    for filename in &meta.trajectory_file_names {
        input_files.push(filename.to_string());
    }

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
#[cfg(test)]
mod tests {
    use super::*;
    use libmdrepo::metadata::Meta;
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    const MINIMAL_TOML: &str = r#"
        lead_contributor_orcid = "0000-0000-0000-0000"
        trajectory_file_names = ["traj.xtc"]
        structure_file_name = "struct.pdb"
        topology_file_name = "top.top"
        temperature_kelvin = 300
        integration_timestep_fs = 2
        short_description = "A test simulation"
        software_name = "GROMACS"
        software_version = "2023"
    "#;

    #[test]
    fn test_round_dp() {
        assert_eq!(round_dp(1.23456, 2), 1.23);
        assert_eq!(round_dp(1.23456, 3), 1.235);
        assert_eq!(round_dp(1.5, 0), 2.0);
        assert_eq!(round_dp(-1.23456, 2), -1.23);
    }

    #[test]
    fn topology_hash_is_deterministic() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "topology content").unwrap();
        let h1 = get_file_hash(f.path()).unwrap();
        let h2 = get_file_hash(f.path()).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn topology_hash_differs_for_different_content() {
        let mut f1 = NamedTempFile::new().unwrap();
        let mut f2 = NamedTempFile::new().unwrap();
        write!(f1, "content A").unwrap();
        write!(f2, "content B").unwrap();
        assert_ne!(
            get_file_hash(f1.path()).unwrap(),
            get_file_hash(f2.path()).unwrap()
        );
    }

    #[test]
    fn topology_hash_missing_file_errors() {
        assert!(get_file_hash(Path::new("/nonexistent")).is_err());
    }

    #[test]
    fn unique_file_hash_is_deterministic() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("struct.pdb"), b"structure").unwrap();
        fs::write(dir.path().join("top.top"), b"topology").unwrap();
        fs::write(dir.path().join("traj.xtc"), b"trajectory").unwrap();
        let meta = Meta::from_toml(MINIMAL_TOML).unwrap();
        let h1 = get_unique_file_hash(&meta, dir.path());
        let h2 = get_unique_file_hash(&meta, dir.path());
        assert_eq!(h1, h2);
    }

    #[test]
    fn unique_file_hash_changes_with_content() {
        let dir_a = tempdir().unwrap();
        fs::write(dir_a.path().join("struct.pdb"), b"structure A").unwrap();
        fs::write(dir_a.path().join("top.top"), b"topology A").unwrap();
        fs::write(dir_a.path().join("traj.xtc"), b"trajectory A").unwrap();

        let dir_b = tempdir().unwrap();
        fs::write(dir_b.path().join("struct.pdb"), b"structure B").unwrap();
        fs::write(dir_b.path().join("top.top"), b"topology B").unwrap();
        fs::write(dir_b.path().join("traj.xtc"), b"trajectory B").unwrap();

        let meta = Meta::from_toml(MINIMAL_TOML).unwrap();
        assert_ne!(
            get_unique_file_hash(&meta, dir_a.path()),
            get_unique_file_hash(&meta, dir_b.path()),
        );
    }

    #[test]
    fn unique_file_hash_skips_missing_files() {
        // Only struct.pdb exists; topology and trajectory are absent.
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("struct.pdb"), b"structure").unwrap();
        let meta = Meta::from_toml(MINIMAL_TOML).unwrap();
        // Should not panic or error — missing files are silently dropped.
        let hash = get_unique_file_hash(&meta, dir.path());
        assert!(!hash.is_empty());
    }

    fn write_blast_tsv(dir: &Path, db: &str, rows: &[(u32, &str, f64)]) {
        let content = rows
            .iter()
            .map(|(qaccver, saccver, pident)| {
                format!("{qaccver}\t{saccver}\t{pident}\t150\t0\t0\t1\t150\t1\t150\t1e-80\t300.0")
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(dir.join(format!("blast.{db}.tsv")), content).unwrap();
    }

    #[test]
    fn blast_uniprot_parses_swissprot_hit() {
        let tmp = tempdir().unwrap();
        let blast_dir = tempdir().unwrap();
        let fasta = tmp.path().join("sequence.fa");
        fs::write(&fasta, b">1\nACGT").unwrap();
        write_blast_tsv(
            tmp.path(),
            "swissprot",
            &[(1, "sp|P12345|PROT_HUMAN", 100.0)],
        );
        let ids =
            blast_uniprot(&fasta, blast_dir.path(), UniprotDb::Swissprot).unwrap();
        assert_eq!(ids, vec!["P12345".to_string()]);
    }

    #[test]
    fn blast_uniprot_parses_trembl_hit() {
        let tmp = tempdir().unwrap();
        let blast_dir = tempdir().unwrap();
        let fasta = tmp.path().join("sequence.fa");
        fs::write(&fasta, b">1\nACGT").unwrap();
        write_blast_tsv(
            tmp.path(),
            "trembl",
            &[(1, "tr|A0A000XYZ|PROT_MOUSE", 100.0)],
        );
        let ids = blast_uniprot(&fasta, blast_dir.path(), UniprotDb::Trembl).unwrap();
        assert_eq!(ids, vec!["A0A000XYZ".to_string()]);
    }

    #[test]
    fn blast_uniprot_excludes_low_pident() {
        let tmp = tempdir().unwrap();
        let blast_dir = tempdir().unwrap();
        let fasta = tmp.path().join("sequence.fa");
        fs::write(&fasta, b">1\nACGT").unwrap();
        write_blast_tsv(
            tmp.path(),
            "swissprot",
            &[
                (1, "sp|P99999|GOOD_HUMAN", 100.0),
                (1, "sp|P00001|POOR_HUMAN", 95.0),
            ],
        );
        let ids =
            blast_uniprot(&fasta, blast_dir.path(), UniprotDb::Swissprot).unwrap();
        assert_eq!(ids, vec!["P99999".to_string()]);
    }

    #[test]
    fn blast_uniprot_rejects_nonexistent_blast_dir() {
        let tmp = tempdir().unwrap();
        let fasta = tmp.path().join("sequence.fa");
        fs::write(&fasta, b">1\nACGT").unwrap();
        assert!(blast_uniprot(
            &fasta,
            Path::new("/nonexistent/blast"),
            UniprotDb::Swissprot
        )
        .is_err());
    }
}
