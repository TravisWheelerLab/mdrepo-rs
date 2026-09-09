use crate::{
    import::{self, ImportOpts},
    ticket::dsn_for,
    types::{
        BlastResult, CheckedLigand, DoiAuthor, DoiPaper, Duration, Export,
        ExportSimulation, ImportJsonArgs, ImportResult, InferredLigand, MdFile,
        PdbEntry, PdbResponse, ProcessArgs, ProcessResult, ProcessTrajectoryArgs,
        ProcessedTarball, ProcessedTrajectory, ProcessedTrajectoryType, PushOutcome,
        RmsdRmsf, RunImportArgs, Server, UniprotDb, UniprotEntry, UniprotResponse,
    },
    validate,
};
use anyhow::{Result, anyhow, bail};
use dotenvy::dotenv;
use libmdrepo::{
    common::{file_exists, get_md5, read_file},
    constants,
    metadata::{self, Meta, MetaCheckOptions},
};
use log::debug;
use rayon::prelude::*;
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    collections::{HashMap, HashSet},
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
const BLAST_MAX_TARGET_SEQS_SWISSPROT: &str = "20";
const BLAST_MAX_TARGET_SEQS_TREMBL: &str = "100";
const BLAST_MIN_PIDENT: f64 = 100.0;

// ── Unit conversions ──────────────────────────────────────────────────────────
const FS_PER_PS: f64 = 1000.0;
const PS_PER_NS: f64 = 1000.0;
const XTC_INFLATION_FACTOR: f64 = 1000.0;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<ProcessResult> {
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

    if !meta_path.is_file() {
        bail!(r#"Missing TOML metadata "{}""#, meta_path.display());
    }

    // Read the metadata and canonicalize its ligand SMILES in memory: OpenBabel
    // normalises non-standard notation (e.g. [N+H3]) to the form the validator
    // accepts ([NH3+]) without the submitter's TOML ever being rewritten. This
    // `meta` is reused for validation and everything downstream, so the
    // validator and the database see identical canonical forms from one read.
    let meta = validate::load_canonical_meta(&meta_path, &script_dir, &uv)?;

    // Validate
    let meta_check_opts = args.no_id.then_some(MetaCheckOptions {
        allow_no_pdb_uniprot: true,
    });

    // Check input files
    match validate::validate_meta(&input_dir, &meta, meta_check_opts) {
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

    let mut trajectory_file_names = meta.trajectory_file_names.clone();
    trajectory_file_names.sort();

    // Resolve the simproc env prefix once (serially) so per-trajectory work can
    // invoke its python directly rather than via `micromamba run`.
    let simproc_prefix = find_conda_env_prefix("simproc")?;
    debug!("Using simproc env prefix: {}", simproc_prefix.display());

    let trajectory_start = Instant::now();
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
    let trajectory_processing_elapsed = trajectory_start.elapsed();

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

    // The processed trajectories are returned sorted by full xtc size and filename
    let example_trajectory = processed_trajectories
        .last()
        .ok_or_else(|| anyhow!("Unable to get last of processed trajectories"))?;

    let trajectory_tarballs =
        make_trajectory_tarballs(&processed_dir, &processed_trajectories)?;

    let import_json = &processed_dir.join("import.json");
    let (simulation, warnings, blast_elapsed) = make_import_json(ImportJsonArgs {
        meta,
        import_json,
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
        blast_num_threads: args.blast_num_threads,
    })?;

    let mut simulation_id: Option<u32> = None;
    let mut push_elapsed = None;
    if !args.dry_run {
        let imported_id = run_import(RunImportArgs {
            simulation: &simulation,
            server: &args.server,
            reprocess_simulation_id: args.reprocess_simulation_id,
            replace_original_files: args.replace_original_files,
            ticket_id: args.ticket_id,
        })?;
        simulation_id = Some(imported_id);

        let import_result = ImportResult {
            filename: import_json.to_string_lossy().to_string(),
            simulation_id: imported_id,
        };
        debug!("{import_result:?}");

        let push_start = Instant::now();
        let push_outcome = run_push(
            &uv,
            &script_dir,
            import_result,
            &input_dir,
            &args.server.to_string(),
            args.reprocess_simulation_id,
            &processed_dir,
            args.transfer_threads,
        )?;
        push_elapsed = Some(push_start.elapsed());
        debug!("{push_outcome:?}");

        // The push script no longer touches the DB. Flip is_placeholder here
        // from the verified outcome: complete clears it, incomplete leaves it
        // set. This runs on both paths so the row always reflects what is
        // actually on the remotes, and it is idempotent across reprocesses
        // (import re-sets is_placeholder=true at the start of every run).
        set_placeholder(&args.server, imported_id, !push_outcome.complete)?;

        if !push_outcome.complete {
            let unverified: Vec<String> = push_outcome
                .files
                .iter()
                .filter(|f| !f.verified)
                .map(|f| {
                    let why = if f.present { "md5 mismatch" } else { "missing" };
                    format!("{} [{}] {why}", f.dest, f.location)
                })
                .collect();

            let mut msg = format!(
                "Push incomplete for simulation {imported_id}: \
                 {} of {} file(s) unverified",
                unverified.len(),
                push_outcome.files.len(),
            );
            if !unverified.is_empty() {
                msg.push_str(&format!("\n{}", unverified.join("\n")));
            }
            if !push_outcome.errors.is_empty() {
                msg.push_str(&format!(
                    "\nUpload errors:\n{}",
                    push_outcome.errors.join("\n")
                ));
            }
            bail!(msg);
        }
    }

    debug!("Finished processing in {:?}", start_time.elapsed());

    Ok(ProcessResult {
        simulation_id,
        warnings,
        trajectory_processing_elapsed,
        blast_elapsed,
        push_elapsed,
    })
}

// --------------------------------------------------
/// Import the simulation into the database, returning its ID. Unlike the ticket
/// feedback path, a DB failure here is fatal — the import *is* the work.
fn run_import(args: RunImportArgs) -> Result<u32> {
    let env_key = dsn_for(args.server);
    let url = env::var(env_key).map_err(|e| anyhow!("{env_key}: {e}"))?;
    let mut conn =
        mdr_db::connect(&url).map_err(|e| anyhow!("Failed to connect: {e}"))?;

    let sim_id = import::import_simulation(
        &mut conn,
        args.simulation,
        &ImportOpts {
            reprocess_simulation_id: args.reprocess_simulation_id,
            replace_original_files: args.replace_original_files,
            ticket_id: args.ticket_id,
        },
    )?;

    u32::try_from(sim_id).map_err(|e| anyhow!("Simulation ID {sim_id}: {e}"))
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
    transfer_threads: usize,
) -> Result<PushOutcome> {
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
        "--transfer-threads".to_string(),
        transfer_threads.to_string(),
    ];

    if reprocess_simulation_id.is_some() {
        args.push("--remove-processed-dir".to_string());
    }

    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir).args(&args);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(
            "Command failed ({}): {cmd:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if !file_exists(&out_file) {
        let stdout = str::from_utf8(&output.stdout)?;
        bail!(r#"Failed to create "{}" ({stdout})"#, out_file.display());
    }

    serde_json::from_str(&read_file(&out_file)?)
        .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, out_file.display()))
}

// --------------------------------------------------
/// Set `is_placeholder` for a simulation from the verified push outcome. Opens
/// its own connection (mirroring `run_import`) and touches only this one column.
fn set_placeholder(server: &Server, sim_id: u32, is_placeholder: bool) -> Result<()> {
    let env_key = dsn_for(server);
    let url = env::var(env_key).map_err(|e| anyhow!("{env_key}: {e}"))?;
    let mut conn =
        mdr_db::connect(&url).map_err(|e| anyhow!("Failed to connect: {e}"))?;

    mdr_db::ops::update_simulation(
        &mut conn,
        i64::from(sim_id),
        mdr_db::models::SimulationUpdate {
            is_placeholder: Some(is_placeholder),
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("Failed to set is_placeholder for {sim_id}: {e}"))?;

    Ok(())
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
        let mut cmd = Command::new(uv);
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
            bail!(
                "Command failed ({}): {cmd:?}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
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
        .ok_or_else(|| {
            anyhow!("Unexpected output from `micromamba env list --json`")
        })?;
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

    // None unless this run actually converted the trajectory: the arm below
    // that finds the outputs already present never reads the source, so it
    // cannot know, and must not claim to.
    let mut source_has_time_axis: Option<bool> = None;

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
                bail!(
                    "Command failed ({}): {cmd:?}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
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
            bail!(
                "Command failed ({}): {cmd:?}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let missing: Vec<_> = full_min_files
            .iter()
            .filter(|f| !file_exists(f))
            .map(|f| f.to_string_lossy().to_string())
            .collect();

        if !missing.is_empty() {
            // A missing full/minimal output is fatal: the steps below require
            // these files, and a partial result must never be recorded as a
            // success. The command exited 0 but produced nothing, so the reason
            // (if any) is usually on stdout -- include both streams inline.
            bail!(
                "Command succeeded but failed to create {}: {cmd:?}\n{}\n{}",
                missing.join(", "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }

        // The wrapper reports what the SOURCE trajectory held, before this
        // conversion rewrote its timing. Read it here or not at all.
        source_has_time_axis =
            parse_time_axis_marker(&String::from_utf8_lossy(&output.stdout));
        debug!("Source time axis: {source_has_time_axis:?}");
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
    let is_coarse_grained = check_coarse_grained(&full_pdb, &min_pdb)?;

    sample_trajectory(&min_xtc, &min_pdb, &sampled_xtc, args.script_dir, args.uv)?;

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
        source_has_time_axis,
    })
}

// --------------------------------------------------
pub fn check_coarse_grained(full_pdb: &Path, min_pdb: &Path) -> Result<bool> {
    // TEMPORARY FIX TO REMOVE REMARK UNTIL KEN'S PR IS ACCEPTED BY pdbrust
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
        fixed.to_file(path)?;
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
        let mut cmd = Command::new(uv);
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
            bail!(
                "Command failed ({}): {cmd:?}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
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
    num_threads: usize,
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

        // blastp takes -num_threads as a string arg; render it once here.
        let num_threads = num_threads.to_string();
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
            num_threads.as_str(),
            "-max_target_seqs",
            max_target_seqs,
            "-task",
            "blastp-fast",
            "-comp_based_stats",
            "0",
        ]);
        debug!("Running {cmd:?}");

        let output = cmd.output()?;

        debug!("{}", str::from_utf8(&output.stdout)?);

        if !output.status.success() {
            bail!(
                "Command failed ({}): {cmd:?}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
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
        let mut cmd = Command::new(uv);
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
            bail!(
                "Command failed ({}): {cmd:?}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
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
        let mut cmd = Command::new(uv);
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
            bail!(
                "Command failed ({}): {cmd:?}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
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
                bail!(
                    "Command failed ({}): {cmd:?}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
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
/// Build the simulation payload, write it to `import.json`, and return it along
/// with the warnings collected on the way.
pub fn make_import_json(
    args: ImportJsonArgs,
) -> Result<(ExportSimulation, Vec<String>, std::time::Duration)> {
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
        args.meta.sampling_frequency_ps,
        args.processed_dir,
    )?;

    let inferred_ligands = get_inferred_ligands(
        &args.example_trajectory.min_pdb,
        args.processed_dir,
        args.script_dir,
        args.uv,
    )?;

    let unique_file_hash_string = get_unique_file_hash(&args.meta, args.input_dir);

    let blast_start = Instant::now();
    let (uniprots, uniprot_warnings) = get_uniprot_entries(
        args.meta.uniprot_ids.clone(),
        &fasta_sequence_file,
        args.blast_dir,
        args.blast_num_threads,
    )?;
    let blast_elapsed = blast_start.elapsed();

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
        sampling_frequency_ps: args.meta.sampling_frequency_ps,
        integration_timestep_fs: args.meta.integration_timestep_fs,
        external_links: args.meta.external_links.unwrap_or_default(),
        collections: args.meta.collections.unwrap_or_default(),
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

    let export = Export { simulation };

    debug!(r#"Writing JSON to "{}""#, args.import_json.display());
    let file = File::create(args.import_json)?;
    writeln!(&file, "{}", &serde_json::to_string_pretty(&export)?)?;

    Ok((export.simulation, warnings, blast_elapsed))
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
                        given_ligand.identity()
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
                // mol_id.py reports a SMILES and an InChIKey but no InChI, so
                // there is nothing to put here; resolution derives it.
                smiles: Some(ligand.structure.smiles),
                inchi: None,
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
                bail!(
                    "Command failed ({}): {cmd:?}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
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
    blast_num_threads: usize,
) -> Result<(Vec<UniprotEntry>, Vec<String>)> {
    let mut uniprot_ids: Vec<String> = given_uniprot_ids
        .unwrap_or_default()
        .iter()
        .map(|id| id.to_uppercase())
        .collect();
    let mut warnings = vec![];

    if uniprot_ids.is_empty() {
        // There are no given Uniprot IDs, so search
        let swissprot_ids = blast_uniprot(
            fasta_sequence_file,
            blast_dir,
            UniprotDb::Swissprot,
            blast_num_threads,
        )?;

        if swissprot_ids.is_empty() {
            // Second-tier hits from Trembl
            let trembl_ids = blast_uniprot(
                fasta_sequence_file,
                blast_dir,
                UniprotDb::Trembl,
                blast_num_threads,
            )?;

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
        let swissprot_ids = blast_uniprot(
            fasta_sequence_file,
            blast_dir,
            UniprotDb::Swissprot,
            blast_num_threads,
        )?;

        let not_in_swissprot: Vec<_> = uniprot_ids
            .iter()
            .filter(|id| !swissprot_ids.contains(id))
            .collect();

        if !not_in_swissprot.is_empty() {
            let isoform_ids = blast_uniprot(
                fasta_sequence_file,
                blast_dir,
                UniprotDb::Isoform,
                blast_num_threads,
            )?;

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
                let trembl_ids = blast_uniprot(
                    fasta_sequence_file,
                    blast_dir,
                    UniprotDb::Trembl,
                    blast_num_threads,
                )?;

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
pub fn check_ligand(
    ligand: &metadata::Ligand,
    inferred_ligand: &InferredLigand,
    script_dir: &Path,
    uv: &Path,
) -> Result<CheckedLigand> {
    // Comparison is SMILES-to-SMILES because the inference produces a SMILES.
    //
    // A `None` here means the submitter declared only an `inchi`. Nothing
    // derives the missing notation -- see the note in import.rs's
    // upsert_ligand -- so this refuses rather than comparing nothing.
    let smiles = ligand.smiles.as_deref().ok_or_else(|| {
        anyhow!(
            r#"Ligand "{}" has no SMILES to compare; it was not resolved"#,
            ligand.name
        )
    })?;

    let script = script_dir.join("compare_smiles.py");
    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir).args([
        "run",
        script.to_string_lossy().as_ref(),
        smiles,
        &inferred_ligand.structure.smiles,
    ]);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(
            "Command failed ({}): {cmd:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
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
        let mut cmd = Command::new(uv);
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
/// Read the wrapper's `[mdrepo] source_has_time_axis=` line
///
/// Returns None when the marker is absent or unrecognised, which is the same
/// thing the wrapper means by "unknown": we could not establish that the
/// source lacked a time axis, so nothing downstream may act as though we had.
fn parse_time_axis_marker(stdout: &str) -> Option<bool> {
    const MARKER: &str = "[mdrepo] source_has_time_axis=";
    stdout
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(MARKER))
        .and_then(|value| match value.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

// --------------------------------------------------
pub fn get_duration(
    trajectories: &[ProcessedTrajectory],
    example_full_xtc: &Path,
    integration_timestep_fs: u32,
    declared_sampling_ps: Option<f64>,
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
            // The second element is measure_trajectory's own sampling figure,
            // rounded to 3 dp in ns. Too coarse for a floor whose whole range
            // of interest is 1 to 10 ps, so the spacing is recomputed below.
            let (measured_ps, _measured_ns, num_frames) =
                measure_trajectory(&traj.full_xtc, integration_timestep_fs)?;

            // What was measured is only trustworthy if the source had a time
            // axis to begin with. When it did not, conversion stamped
            // cpptraj's default 1 ps/frame onto full.xtc and `measured_ns` is
            // that default wearing the costume of a measurement -- which is
            // exactly how MDR00048669 came to record 0.001 ns/frame and a
            // 5.61 ns duration, both a clean factor of ten too small, and sit
            // in the public record for a year looking authoritative.
            //
            // What comes out of the match is the frame spacing in ps, whatever
            // established it. Duration and sampling frequency are both derived
            // from it afterwards, so the floor has one number to check rather
            // than two.
            let spacing_ps = match traj.source_has_time_axis {
                Some(false) => {
                    let declared = declared_sampling_ps.ok_or_else(|| {
                        anyhow!(
                            "{}: the source trajectory carries no time axis \
                             and the metadata declares no \
                             `sampling_frequency_ps`, so the frame spacing \
                             cannot be established. Converting anyway would \
                             record cpptraj's default of 1 ps per frame as \
                             though it had been measured. Add \
                             `sampling_frequency_ps` to mdrepo-metadata.toml \
                             (the output frequency times the integration \
                             timestep) and re-run.",
                            traj.trajectory_file_name
                        )
                    })?;

                    debug!(
                        "{}: no source time axis; using declared \
                         {declared} ps/frame over {num_frames} frames",
                        traj.trajectory_file_name
                    );
                    declared
                }
                // True, or unknown. Unknown keeps today's behaviour on
                // purpose: it means the report could not be read, not that
                // the axis is absent, and failing on that would condemn
                // directories over a parsing gap.
                //
                // measure_trajectory returns a DURATION, so divide back out to
                // the spacing the floor is expressed in.
                _ => measured_ps / (num_frames - 1.),
            };

            let spacing_ps = resolve_sampling_ps(
                spacing_ps,
                declared_sampling_ps,
                traj.source_has_time_axis,
                &traj.trajectory_file_name,
            )?;

            // Spacing between frames, so the span is one interval fewer than
            // the frame count.
            let duration_ps = spacing_ps * (num_frames - 1.);
            let sampling_ns = round_dp(spacing_ps / PS_PER_NS, 3) as f32;

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

// --------------------------------------------------
/// Reconcile a frame spacing against the sampling floor. Returns the spacing to
/// use, in ps.
///
/// Four outcomes, in the order they are tested:
///
/// 1. Below `SAMPLING_FREQUENCY_PS_MIN` -- refused outright, whatever produced
///    it. 1 ps is the absolute minimum spacing MDRepo will record, and the five
///    prod simulations that violate it all record exactly 0.
/// 2. The source had no time axis, so `spacing_ps` IS the declaration. There is
///    no measurement to corroborate it against; it stands, as it has since
///    2026-08-11.
/// 3. At or above `SAMPLING_FLOOR_PS` -- the measurement stands on its own and
///    any declaration is ignored, which is the path essentially every
///    simulation takes.
/// 4. Below the floor and measured -- the submitter must declare the same
///    value. A spacing this small is not by itself wrong, but neither is it
///    self-evidently right: a trajectory carrying no usable timing is stamped
///    with a converter's default of 1 ps per frame and the fabrication is
///    indistinguishable from the real thing. Requiring agreement means two
///    independent sources say it, which is the most that can be had.
fn resolve_sampling_ps(
    spacing_ps: f64,
    declared_ps: Option<f64>,
    source_had_time_axis: Option<bool>,
    trajectory_file_name: &str,
) -> Result<f64> {
    if spacing_ps < constants::SAMPLING_FREQUENCY_PS_MIN {
        bail!(
            "{trajectory_file_name}: the frame spacing works out to \
             {spacing_ps} ps, below the {} ps minimum. No simulation here \
             records frames closer together than that, and a value this small \
             is a fabricated or corrupt time axis rather than a real sampling \
             rate. It cannot be declared around: correct the trajectory's \
             timing, or re-upload with a file that carries it.",
            constants::SAMPLING_FREQUENCY_PS_MIN
        )
    }

    // The declared path. Nothing measured it, so there is nothing to compare
    // it with -- see get_duration's Some(false) arm, which produced this value
    // from `sampling_frequency_ps` in the first place.
    if source_had_time_axis == Some(false) {
        return Ok(spacing_ps);
    }

    if spacing_ps >= constants::SAMPLING_FLOOR_PS {
        if let Some(declared) = declared_ps {
            debug!(
                "{trajectory_file_name}: measured {spacing_ps} ps/frame is at \
                 or above the {} ps floor, so the declared \
                 sampling_frequency_ps of {declared} ps is not consulted",
                constants::SAMPLING_FLOOR_PS
            );
        }
        return Ok(spacing_ps);
    }

    let Some(declared) = declared_ps else {
        bail!(
            "{trajectory_file_name}: the frame spacing measures {spacing_ps} \
             ps, below the {} ps minimum for a value derived from the \
             trajectory. A spacing this small is usually fabricated -- a file \
             carrying no time axis of its own is stamped with a converter's \
             default of 1 ps per frame, which reads exactly like a real \
             measurement -- so it is not recorded on the trajectory's word \
             alone. If this simulation genuinely did save frames that close \
             together, declare it: add `sampling_frequency_ps = {spacing_ps}` \
             to mdrepo-metadata.toml and re-run.",
            constants::SAMPLING_FLOOR_PS
        )
    };

    if (spacing_ps - declared).abs()
        > constants::SAMPLING_AGREEMENT_TOLERANCE * declared
    {
        bail!(
            "{trajectory_file_name}: mdrepo-metadata.toml declares \
             `sampling_frequency_ps = {declared}`, but the trajectory measures \
             {spacing_ps} ps per frame. Below the {} ps floor the declaration \
             has to corroborate the file rather than overrule it, so the two \
             must agree. If the trajectory's own timing is the part that is \
             wrong -- a converter can write a placeholder time axis that \
             cannot be recovered from -- then the file needs correcting; the \
             declaration cannot stand in for it.",
            constants::SAMPLING_FLOOR_PS
        )
    }

    debug!(
        "{trajectory_file_name}: measured {spacing_ps} ps/frame is below the \
         {} ps floor and agrees with the declared {declared} ps; using the \
         declared value",
        constants::SAMPLING_FLOOR_PS
    );
    Ok(declared)
}

/// How many significant digits a frame-time gap keeps before it is counted.
/// Finer than any tolerance a caller compares against, coarse enough to absorb
/// f32 jitter in the stamps themselves.
const GAP_SIGNIFICANT_DIGITS: i32 = 4;

/// `molly --times` is a filtering mode and insists on an output path. The
/// filtered trajectory is not wanted, only the times it prints on the way.
const MOLLY_TIMES_SINK: &str = "/dev/null";

/// Every frame time (ps) of a trajectory, in file order, via `molly --times`.
///
/// `molly --info` reports only the first and last time, which is not enough --
/// see [`modal_gap_ps`] for why the whole series is needed.
fn frame_times(full_xtc: &Path) -> Result<Vec<f64>> {
    let mut cmd = Command::new("molly");
    cmd.args([
        "--times",
        full_xtc.to_string_lossy().as_ref(),
        MOLLY_TIMES_SINK,
    ]);
    debug!("Running {cmd:?}");
    let output = cmd.output()?;

    if !output.status.success() {
        bail!(
            "Command failed ({}): {cmd:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    parse_frame_times(str::from_utf8(&output.stdout)?, full_xtc)
}

/// Parse `molly --times` output: one frame time in ps per line.
///
/// molly writes a tab-separated row and leaves the `--steps` column out when it
/// was not asked for, so every line carries a trailing tab.
fn parse_frame_times(stdout: &str, full_xtc: &Path) -> Result<Vec<f64>> {
    let mut times = Vec::new();
    for line in stdout.lines() {
        let field = line.split('\t').next().unwrap_or_default().trim();
        if field.is_empty() {
            continue;
        }
        times.push(field.parse::<f64>().map_err(|e| {
            anyhow!(
                "Failed to parse frame time {field:?} for \"{}\": {e}",
                full_xtc.display()
            )
        })?);
    }
    Ok(times)
}

/// Bucket a gap by its leading [`GAP_SIGNIFICANT_DIGITS`] digits, as
/// (exponent, mantissa) so the key is exact integer arithmetic rather than
/// float equality.
fn gap_bucket(gap: f64) -> (i32, i64) {
    let mut exp = gap.log10().floor() as i32;
    let mut mantissa =
        (gap / 10f64.powi(exp - (GAP_SIGNIFICANT_DIGITS - 1))).round() as i64;
    // Rounding can carry the mantissa past its width (9999.6 -> 10000), which
    // would otherwise drop two all-but-identical gaps into separate buckets on
    // either side of a power of ten.
    if mantissa >= 10i64.pow(GAP_SIGNIFICANT_DIGITS as u32) {
        mantissa /= 10;
        exp += 1;
    }
    (exp, mantissa)
}

/// The most common positive gap between consecutive frame times, in ps.
///
/// Deliberately the mode, and deliberately not `(last - first) / (n - 1)`: some
/// releases ship trajectories that are concatenations of several shorter
/// segments, and the frame clock restarts at each boundary. Spanning
/// first..last then measures whichever segment happens to end the file instead
/// of the run. BioEmu's ONE-cath1 `cath1_1b43A02/run006_protein.cmprsd.xtc`
/// holds 501 frames 10000 ps apart and reports `time: 0-30000 ps`, understating
/// that file by two orders of magnitude and its 25-trajectory bundle by 39%.
/// Taking the mode over consecutive gaps and ignoring the non-positive ones
/// drops the boundaries on the floor and recovers the spacing that actually
/// holds inside a segment.
///
/// Gaps are bucketed before counting because xtc frame times are f32: at
/// multi-microsecond timestamps two nominally equal gaps can differ by a
/// fraction of a ps (the ULP at t = 2 us is 0.125), and that jitter would
/// otherwise split one spacing across neighbouring buckets. The median of the
/// winning bucket is returned, so the result is a gap the file really contains
/// rather than a rounded stand-in.
///
/// Returns `None` when no pair of frames is ordered in time, which for a
/// trajectory of two frames or more means the stamps are unusable.
fn modal_gap_ps(times: &[f64]) -> Option<f64> {
    // Insertion-ordered, so equally populated buckets resolve to the spacing
    // seen first rather than to whatever the hasher happens to surface.
    let mut order: Vec<Vec<f64>> = Vec::new();
    let mut seen: HashMap<(i32, i64), usize> = HashMap::new();
    for pair in times.windows(2) {
        let gap = pair[1] - pair[0];
        if gap <= 0. {
            continue; // a segment boundary, not a sampling interval
        }
        let slot = *seen.entry(gap_bucket(gap)).or_insert_with(|| {
            order.push(Vec::new());
            order.len() - 1
        });
        order[slot].push(gap);
    }

    let mut winner = order.into_iter().max_by_key(|gaps| gaps.len())?;
    winner.sort_by(f64::total_cmp);
    Some(winner[winner.len() / 2])
}

/// Measure one trajectory with `molly --times`, returning
/// (duration_ps, sampling_frequency_ns, num_frames). Includes the 1000x
/// inflated-timestamp correction used historically.
fn measure_trajectory(
    full_xtc: &Path,
    integration_timestep_fs: u32,
) -> Result<(f64, f32, f64)> {
    let times = frame_times(full_xtc)?;
    let num_frames = times.len() as f64;

    if num_frames <= 1. {
        bail!(
            "Trajectory file \"{}\" has only {num_frames} frame(s)",
            full_xtc.display()
        );
    }

    // The spacing is measured, then the span is derived from it, rather than
    // the other way round: a trajectory built by concatenating segments has no
    // single span to measure, but every segment shares one spacing.
    let sampling_ps = modal_gap_ps(&times).ok_or_else(|| {
        anyhow!(
            "Trajectory file \"{}\" has {num_frames} frames but no pair of \
             them advances in time, so the frame spacing cannot be measured",
            full_xtc.display()
        )
    })?;
    let mut duration_ps = (num_frames - 1.) * sampling_ps;

    // Sanity check: compute nstxout (output steps per frame)
    // using the integration timestep from metadata.
    let nstxout = sampling_ps / (integration_timestep_fs as f64 / FS_PER_PS);

    // Only the UPPER bound is a check, and deliberately so. If nstxout is way
    // too large but dividing by 1000 fixes it, the XTC timestamps are inflated
    // by 1000x (a known issue with some MD engines).
    //
    // The 1e3 below only confirms that correction landed somewhere sensible.
    // It is NOT a floor on nstxout and must not become one: the Dissociation
    // Dynamic Database saved every 1 ps on a 1 fs timestep, which is 1,000
    // steps per frame -- sitting exactly ON 1e3, with no margin either way.
    //
    // That timestep read 2 fs / 500 steps until 2026-09-09 and was wrong;
    // measured, all 15,525 files in the 2026-09-08 delivery and all 6,664
    // imported originals declare 1 fs. So the original claim that real data
    // sits UNDER this bound does not hold -- it sits on it, and an inclusive
    // lower bound would admit the DDD rather than reject it. The bound still
    // must not be added: a margin of zero is no margin, one rounding either way
    // decides it, and this function cannot see the declaration that authorises
    // the value. Implausibly fine sampling is caught by resolve_sampling_ps
    // instead, in ps, where the declaration is in scope.
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

    Ok((duration_ps, sampling_frequency_ns, num_frames))
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

    Ok(mk_paper(doi_paper, doi))
}

// --------------------------------------------------
/// Map a Crossref/DOI record onto a `Paper`. This is the authority on how a DOI
/// becomes a publication; `utils/python/fix_pub_metadata.py`, which backfills
/// the rows the retired Python importer wrote, mirrors it.
fn mk_paper(doi_paper: DoiPaper, doi: &str) -> metadata::Paper {
    let authors = doi_paper
        .author
        .iter()
        .filter_map(DoiAuthor::display_name)
        .collect::<Vec<String>>()
        .join(", ");

    // Crossref spreads the publication date across four fields and populates
    // whichever ones it has; take the year from the first that carries a date.
    let year = [
        doi_paper.published,
        doi_paper.issued,
        doi_paper.published_print,
        doi_paper.published_online,
    ]
    .into_iter()
    .flatten()
    .find_map(|date| {
        date.date_parts
            .first()
            .and_then(|parts| parts.first().copied())
    })
    .unwrap_or(0);
    // Crossref reports volume as a string (e.g. "11", but also "11A"); keep the
    // numeric Paper.volume, falling back to 0 when it is absent or non-numeric.
    let volume = doi_paper.volume.and_then(|v| v.parse().ok()).unwrap_or(0);

    // The journal name is Crossref's `container-title` (e.g. "Scientific Data");
    // fall back to any `journal`, then the `publisher`, then "NA". Using the
    // publisher directly here previously mislabelled the journal as, e.g.,
    // "Springer Science and Business Media LLC".
    let journal = doi_paper
        .container_title
        .or(doi_paper.journal)
        .or(doi_paper.publisher)
        .unwrap_or_else(|| "NA".to_string());

    // The issue number is usually reported at the top level, but some records
    // carry it only inside `journal-issue`.
    let number = doi_paper
        .issue
        .or_else(|| doi_paper.journal_issue.and_then(|issue| issue.issue));

    // Journals that number articles rather than paginating them report an
    // `article-number` in place of a `page`.
    let pages = doi_paper.page.or(doi_paper.article_number);

    metadata::Paper {
        title: doi_paper.title,
        authors,
        journal,
        volume,
        number,
        year,
        pages,
        doi: Some(doi.to_string()),
    }
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
    } else if let Some(name) = desc
        .submission_names
        .and_then(|names| names.into_iter().next())
    {
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
    use tempfile::{NamedTempFile, tempdir};

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

    // The record `fix_pub_metadata.py` was written for: an issue and an
    // `article-number` where there is no `page`, both of which get_doi used to
    // discard, leaving `md_pub.number` and `md_pub.pages` empty.
    #[test]
    fn test_mk_paper_crossref() -> anyhow::Result<()> {
        let text = std::fs::read_to_string("tests/inputs/doi-crossref.json")?;
        let doi_paper: DoiPaper = serde_json::from_str(&text)?;
        let paper = mk_paper(doi_paper, "10.1038/s41597-024-04140-z");

        assert_eq!(
            paper.title,
            "mdCATH: A Large-Scale MD Dataset for Data-Driven \
             Computational Biophysics"
        );
        assert_eq!(paper.journal, "Scientific Data");
        assert_eq!(paper.volume, 11);
        assert_eq!(paper.number, Some("1".to_string()));
        assert_eq!(paper.year, 2024);
        assert_eq!(paper.pages, Some("1299".to_string()));
        assert!(paper.authors.starts_with("Antonio Mirarchi, "));
        Ok(())
    }

    // Only `published-print` carries the date here, and the sole author is a
    // consortium. Before, the year fell back to 0 and the record did not parse.
    #[test]
    fn test_mk_paper_print_date_and_consortium() -> anyhow::Result<()> {
        let text = r#"{
            "title": "An older study",
            "author": [{"name": "The MDRepo Consortium"}],
            "container-title": "Journal of Simulation",
            "journal-issue": {"issue": "4"},
            "published-print": {"date-parts": [[1998, 3]]}
        }"#;
        let doi_paper: DoiPaper = serde_json::from_str(text)?;
        let paper = mk_paper(doi_paper, "10.0000/older");

        assert_eq!(paper.year, 1998);
        assert_eq!(paper.authors, "The MDRepo Consortium");
        assert_eq!(paper.number, Some("4".to_string()));
        assert_eq!(paper.pages, None);
        assert_eq!(paper.volume, 0);
        Ok(())
    }

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

    // A well-behaved trajectory: the mode is the only gap there is, so the
    // spacing (and therefore the duration) is unchanged from what
    // (last - first) / (n - 1) used to give.
    #[test]
    fn modal_gap_of_an_evenly_spaced_trajectory_is_the_spacing() {
        let times: Vec<f64> = (0..201).map(|i| i as f64 * 10000.).collect();
        assert_eq!(modal_gap_ps(&times), Some(10000.));
        // (n - 1) * spacing reproduces the span exactly.
        assert_eq!(200. * modal_gap_ps(&times).unwrap(), 2_000_000.);
    }

    // A spacing at or above the floor is returned untouched, which is the path
    // essentially every simulation takes. Asserted so the floor cannot start
    // rewriting values it is supposed to leave alone.
    #[test]
    fn spacing_above_the_floor_is_returned_unchanged() {
        assert_eq!(
            resolve_sampling_ps(100., None, Some(true), "t.xtc").unwrap(),
            100.
        );
        assert_eq!(resolve_sampling_ps(10., None, None, "t.xtc").unwrap(), 10.);
        // A declaration that is not needed is ignored, not an error. The only
        // 73 simulations in prod that declare this field all say 100 ps and
        // all measure 100 ps, so this is the path they take.
        assert_eq!(
            resolve_sampling_ps(100., Some(100.), Some(true), "t.xtc").unwrap(),
            100.
        );
        // ...and it is still ignored when it disagrees, because above the
        // floor the measurement stands on its own.
        assert_eq!(
            resolve_sampling_ps(800., Some(1.), Some(true), "t.xtc").unwrap(),
            800.
        );
    }

    // 1 ps is absolute. The five simulations that violate it in prod -- 231,
    // 4437, 20610, 20614, 20615 -- all record exactly 0, and nothing
    // legitimate sits between 0 and 1 ps.
    #[test]
    fn below_one_ps_is_refused_from_every_source() {
        // Measured, undeclared.
        assert!(resolve_sampling_ps(0., None, Some(true), "t.xtc").is_err());
        // Measured, and declared to match -- agreement does not buy it in.
        assert!(resolve_sampling_ps(0.5, Some(0.5), Some(true), "t.xtc").is_err());
        // Declared on a trajectory with no time axis, where the declaration is
        // normally taken on trust. The floor is checked before that trust is
        // extended, so this route is closed too.
        let err = resolve_sampling_ps(0.001, Some(0.001), Some(false), "t.nc")
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot be declared around"), "{err}");
    }

    // The case the floor exists for: a DCD with no usable header is stamped
    // with cpptraj's default 1 ps/frame, and that fabricated value used to be
    // measured back off the converted file as though it were real. 73 rows in
    // prod carried it. Note the marker is `unknown` for a DCD, not `false`.
    #[test]
    fn fabricated_one_ps_is_refused_without_a_declaration() {
        let err = resolve_sampling_ps(1., None, None, "E279Q-1v4t.dcd")
            .unwrap_err()
            .to_string();
        assert!(err.contains("E279Q-1v4t.dcd"), "{err}");
        assert!(err.contains("sampling_frequency_ps"), "{err}");
        // The message hands the operator the exact line to add, so the measured
        // value has to appear in it and not just the field name.
        assert!(err.contains("sampling_frequency_ps = 1"), "{err}");

        // The same refusal is what every remaining DDD import would hit today:
        // measured 2026-09-09, zero of the 15,525 delivered metadata files and
        // zero of the 6,664 imported originals declare the field at all. That
        // is why simulation-processing 5bb97c6, which injects it, has to deploy
        // with the floor or before it -- never after.
        assert!(resolve_sampling_ps(1., None, None, "Pro_lig1.mdc").is_err());

        // Every bail names its trajectory. In a bundle of 50 Pro_lig*.mdc
        // replicas an unnamed error is unactionable, and the sub-minimum gate's
        // prefix was untested.
        let sub_minimum = resolve_sampling_ps(4.9e-5, None, None, "converted.dcd")
            .unwrap_err()
            .to_string();
        assert!(sub_minimum.starts_with("converted.dcd:"), "{sub_minimum}");
    }

    // ...and the reason it cannot simply be rejected: the same 1 ps, declared,
    // is the real sampling rate of all 7,197 members of the Dissociation
    // Dynamic Database. Same number, opposite verdict, and only the
    // declaration separates them.
    #[test]
    fn declared_one_ps_is_honoured_when_the_trajectory_agrees() {
        assert_eq!(
            resolve_sampling_ps(1., Some(1.), Some(true), "Pro_lig1.xtc").unwrap(),
            1.
        );
        // The declared value becomes the stored one, so a measured 1.0000004
        // is recorded as a clean 1 rather than propagating float noise.
        assert_eq!(
            resolve_sampling_ps(1.0000004, Some(1.), Some(true), "Pro_lig1.xtc")
                .unwrap(),
            1.
        );
    }

    // Below the floor the declaration corroborates the file rather than
    // overruling it, because a declared number is the one thing a fabricated
    // axis can also supply.
    #[test]
    fn declaration_that_contradicts_the_trajectory_is_refused() {
        let err = resolve_sampling_ps(1., Some(9.), Some(true), "t.xtc")
            .unwrap_err()
            .to_string();
        assert!(err.contains("declares"), "{err}");
        assert!(err.contains("measures"), "{err}");

        // Just inside and just outside the 1% tolerance.
        assert!(resolve_sampling_ps(2.01, Some(2.), Some(true), "t.xtc").is_ok());
        assert!(resolve_sampling_ps(2.03, Some(2.), Some(true), "t.xtc").is_err());
    }

    // The no-time-axis path keeps working as it has since 2026-08-11: the
    // declaration is the value and nothing corroborates it, because there is
    // nothing to corroborate it with.
    #[test]
    fn declared_path_needs_no_corroboration_below_the_floor() {
        assert_eq!(
            resolve_sampling_ps(4., Some(4.), Some(false), "t.mdcrd").unwrap(),
            4.
        );
    }

    // --- either notation alone: accepted by validation, refused by the
    // --- pipeline. Both halves asserted, because the gap is the finding.

    fn inchi_only_ligand() -> libmdrepo::metadata::Ligand {
        libmdrepo::metadata::Ligand {
            name: "ethanol".to_string(),
            smiles: None,
            inchi: Some("InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3".to_string()),
        }
    }

    /// Half one: `mdr-meta check` is happy with an InChI-only ligand. This is
    /// what `cba350b` set out to do and it works.
    #[test]
    fn an_inchi_only_ligand_satisfies_validation() {
        let ligand = inchi_only_ligand();

        assert!(validator::Validate::validate(&ligand).is_ok());
        assert_eq!(ligand.declared_identity(), Some("inchi"));
        assert_eq!(ligand.identity(), "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3");
    }

    /// Half two: the pipeline then refuses it, because nothing ever derives the
    /// SMILES that `md_ligand` and the comparison both require.
    ///
    /// The refusal is correct -- comparing against a molecule you do not have
    /// would be worse -- but it means "either notation is accepted" is true at
    /// the metadata layer and false end to end. Asserted so the sentence in the
    /// docs cannot quietly become wrong in the other direction either.
    ///
    /// Its sibling is `upsert_ligand` in import.rs, which fails the same ligand
    /// with "reached import with no SMILES". That one needs a database, so it
    /// is not asserted here.
    #[test]
    fn an_inchi_only_ligand_cannot_be_compared_today() {
        let inferred: InferredLigand = serde_json::from_str(
            r#"{
                "structure": {
                    "smiles": "CCO", "formula": "C2H6O", "num_atoms": 9,
                    "num_heavy_atoms": 3, "charge": 0,
                    "inchikey": "LFQSCWFLJHTTHZ-UHFFFAOYSA-N", "resname": "LIG"
                },
                "name": {"smiles_input": "CCO"}
            }"#,
        )
        .expect("the shape mol_id.py writes into inferred_ligands.json");

        // The SMILES is extracted before anything is spawned, so a `uv` that
        // cannot exist proves the refusal happens first.
        let err = check_ligand(
            &inchi_only_ligand(),
            &inferred,
            Path::new("."),
            Path::new("/nonexistent/uv"),
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("ethanol"), "{err}");
        assert!(err.contains("has no SMILES to compare"), "{err}");
    }

    /// What slice 2 has to make true: an InChI-only ligand reaches the
    /// comparison with a SMILES derived from its InChI.
    ///
    /// Ignored because the resolver does not exist. Written now because the
    /// contract is decided and this is cheaper to read than a design document
    /// -- and because `cargo test` lists ignored tests by name, so it stays
    /// visible. When the resolver lands, delete the attribute.
    #[test]
    #[ignore = "slice 2: no step derives SMILES from a declared InChI yet"]
    fn an_inchi_only_ligand_should_be_resolved_before_the_pipeline_sees_it() {
        let resolved = inchi_only_ligand();

        assert_eq!(
            resolved.smiles.as_deref(),
            Some("CCO"),
            "resolution must fill the missing notation, not refuse the ligand"
        );
        assert_eq!(resolved.declared_identity(), Some("inchi"));
    }

    // The next representable f64 either side of a positive finite value. Both
    // bounds below are `>=` or `<=`, and the only way to prove an inclusive
    // bound is inclusive is to stand one bit outside it.
    fn one_ulp_below(x: f64) -> f64 {
        f64::from_bits(x.to_bits() - 1)
    }

    fn one_ulp_above(x: f64) -> f64 {
        f64::from_bits(x.to_bits() + 1)
    }

    // Exactly 10 ps is ABOVE the floor, so it keeps its measurement. One bit
    // below, the declaration becomes mandatory. Nothing in the six original
    // tests distinguishes `>=` from `>` here, and getting that wrong would push
    // every exactly-10-ps trajectory into the corroboration branch and fail the
    // ones that declare nothing.
    #[test]
    fn the_ten_ps_floor_is_inclusive_and_one_ulp_below_it_is_not() {
        let below = one_ulp_below(constants::SAMPLING_FLOOR_PS);

        assert_eq!(
            resolve_sampling_ps(
                constants::SAMPLING_FLOOR_PS,
                None,
                Some(true),
                "t.xtc"
            )
            .unwrap(),
            10.
        );
        assert!(resolve_sampling_ps(below, None, Some(true), "t.xtc").is_err());

        // And once it is below, the declared value is what comes back -- not
        // the measurement that was one bit short of standing on its own.
        assert_eq!(
            resolve_sampling_ps(below, Some(10.), Some(true), "t.xtc").unwrap(),
            10.
        );
    }

    // Exactly 1 ps is accepted; one bit below it is refused from every source,
    // including the declared path, because that gate is tested before anything
    // else. Turning `<` into `<=` here would reject the entire DDD.
    #[test]
    fn the_one_ps_minimum_is_inclusive_and_one_ulp_below_it_is_not() {
        let min = constants::SAMPLING_FREQUENCY_PS_MIN;
        let below = one_ulp_below(min);

        assert_eq!(
            resolve_sampling_ps(min, Some(min), Some(true), "t.xtc").unwrap(),
            1.
        );

        for axis in [Some(true), Some(false), None] {
            assert!(
                resolve_sampling_ps(below, None, axis, "t.xtc").is_err(),
                "{axis:?} with no declaration"
            );
            assert!(
                resolve_sampling_ps(below, Some(below), axis, "t.xtc").is_err(),
                "{axis:?} declaring the same sub-minimum value"
            );
        }
    }

    // The agreement window is `|spacing - declared| <= 0.01 * declared`, so the
    // edge is inclusive and the denominator is the DECLARATION, not the
    // measurement and not their mean. The original test probes 2.01 and 2.03,
    // neither of which lands on the edge -- and with `declared = 2.` none can,
    // since `2.02 - 2.0` is 0.020000000000000018 in f64.
    //
    // 6.25 is the value that works: `0.01 * 6.25` is exactly 0.0625, and
    // 6.3125 and 6.1875 sit exactly one tolerance either side. Two other
    // obvious candidates do not work and are worth naming so they are not
    // tried again -- `4.04 - 4.0` is 0.040000000000000036, which is over the
    // line rather than on it, and `declared = 100.` can never reach this gate
    // at all because any spacing within 1% of 100 clears the 10 ps floor.
    #[test]
    fn the_one_percent_tolerance_edge_is_inclusive() {
        let declared = 6.25;
        let hi = 6.3125;
        let lo = 6.1875;

        // State the arithmetic, so that changing SAMPLING_AGREEMENT_TOLERANCE
        // breaks this loudly instead of quietly demoting it to an interior case.
        assert_eq!(
            hi - declared,
            constants::SAMPLING_AGREEMENT_TOLERANCE * declared
        );
        assert_eq!(
            declared - lo,
            constants::SAMPLING_AGREEMENT_TOLERANCE * declared
        );

        assert_eq!(
            resolve_sampling_ps(hi, Some(declared), Some(true), "t.xtc").unwrap(),
            declared
        );
        assert_eq!(
            resolve_sampling_ps(lo, Some(declared), Some(true), "t.xtc").unwrap(),
            declared
        );

        assert!(
            resolve_sampling_ps(one_ulp_above(hi), Some(declared), Some(true), "t.xtc")
                .is_err()
        );
        assert!(
            resolve_sampling_ps(one_ulp_below(lo), Some(declared), Some(true), "t.xtc")
                .is_err()
        );
    }

    // Not a bug, and not something to "fix" -- but the first submitter to
    // declare 1.00 against a measured 1.01 will say it is one percent, and it
    // is refused, because `1.01 - 1.0` is 0.010000000000000009 in binary
    // floating point. The tolerance is an f64 comparison, not a decimal one.
    // If that is ever judged too harsh, this is the test to delete on purpose.
    #[test]
    fn a_nominal_one_percent_disagreement_can_still_be_refused() {
        assert!(
            resolve_sampling_ps(1.01, Some(1.), Some(true), "Pro_lig1.mdc").is_err()
        );
        assert_eq!(
            resolve_sampling_ps(
                one_ulp_below(1.01),
                Some(1.),
                Some(true),
                "Pro_lig1.mdc"
            )
            .unwrap(),
            1.
        );
    }

    // Below the floor an agreeing pair resolves to the DECLARATION, so
    // measurement noise never reaches the published record. The original test
    // shows this with 1.0000004, where the two are indistinguishable by eye;
    // these differ visibly and still come back as the declared value.
    #[test]
    fn the_declaration_not_the_measurement_is_what_is_returned() {
        for spacing in [4.02, 3.98] {
            let got =
                resolve_sampling_ps(spacing, Some(4.), Some(true), "t.xtc").unwrap();
            assert_eq!(got.to_bits(), 4.0f64.to_bits(), "spacing {spacing}");
        }
    }

    // A non-positive declaration always bails, because the right-hand side of
    // the tolerance test goes non-positive with it. The outcome is right and
    // the mechanism is accidental, so it is written down here: replacing
    // `TOLERANCE * declared` with `TOLERANCE * declared.abs()` reads like a
    // tidy-up and would start accepting a negative declaration. The real guard
    // is Meta::check's range validation in libmdrepo/src/metadata.rs.
    #[test]
    fn a_nonpositive_declaration_authorises_nothing() {
        for declared in [0., -1., -1e6, f64::NEG_INFINITY] {
            assert!(
                resolve_sampling_ps(2., Some(declared), Some(true), "t.xtc").is_err(),
                "declared {declared}"
            );
        }
    }

    // The no-time-axis gate returns before the floor and the agreement rule
    // are reached, whatever it is handed. The second case matters most: this
    // function does NOT require a declaration on that path -- get_duration
    // does, before it ever calls here -- so moving this gate below the
    // corroboration branch would start failing trajectories that are fine.
    #[test]
    fn gate_two_short_circuits_before_any_corroboration() {
        assert_eq!(
            resolve_sampling_ps(5., Some(999.), Some(false), "t.mdcrd").unwrap(),
            5.
        );
        assert_eq!(
            resolve_sampling_ps(5., None, Some(false), "t.mdcrd").unwrap(),
            5.
        );
    }

    // `unknown` must behave exactly like `Some(true)` everywhere. This is a
    // promise the code makes in a comment and nothing enforced until now, and
    // it is not a corner case: cpptraj prints no `is a ... with ...` line for a
    // DCD, so EVERY DCD arrives here as None. If the two arms ever diverge,
    // every DCD in the archive changes behaviour at once.
    #[test]
    fn unknown_behaves_exactly_like_true_across_the_whole_grid() {
        let spacings = [
            0.5,
            1.0,
            1.005,
            2.0,
            6.25,
            one_ulp_below(constants::SAMPLING_FLOOR_PS),
            10.0,
            100.0,
        ];
        let declarations = [None, Some(1.0), Some(2.0), Some(6.25), Some(100.0)];

        for spacing in spacings {
            for declared in declarations {
                let known = resolve_sampling_ps(spacing, declared, Some(true), "t.xtc");
                let unknown = resolve_sampling_ps(spacing, declared, None, "t.xtc");

                assert_eq!(
                    known.is_ok(),
                    unknown.is_ok(),
                    "spacing {spacing}, declared {declared:?}"
                );
                if let (Ok(a), Ok(b)) = (&known, &unknown) {
                    assert_eq!(a, b, "spacing {spacing}, declared {declared:?}");
                }
            }
        }
    }

    // A hole, asserted as it stands today rather than as it should be. The
    // function refuses a spacing below 1 ps on the way in and then hands back a
    // declared value below 1 ps on the way out, because it never re-checks what
    // it returns. Nothing reaches this in production -- Meta::check's range
    // validation rejects the declaration first, and
    // sampling_frequency_range_is_inclusive_at_both_ends in libmdrepo is what
    // pins that -- so this is a latent hole, not a live one. When it is closed,
    // this test is the one that goes red, and its name says why.
    #[test]
    fn resolve_sampling_ps_does_not_recheck_its_own_return_value() {
        let got = resolve_sampling_ps(1.0, Some(0.995), Some(true), "t.xtc").unwrap();

        assert_eq!(got, 0.995);
        assert!(
            got < constants::SAMPLING_FREQUENCY_PS_MIN,
            "the returned value is below the minimum this function enforces on input"
        );
    }

    // The DDD's own numbers, which are the reason the floor is 10 ps and not
    // higher. Measured 2026-09-09 over the contributor's 2026-09-08 delivery:
    // all 15,525 metadata files declare `integration_timestep_fs = 1`, and all
    // 6,664 imported batch-1 originals agree. So a saved frame every 1 ps is
    // 1,000 integration steps apart.
    //
    // That corrects a figure repeated in two source comments -- constants.rs's
    // note on SAMPLING_FLOOR_PS and measure_trajectory's note on nstxout -- both
    // of which say 2 fs and 500 steps. The conclusion they draw survives the
    // correction, since 1,000 and 250 are both clean whole numbers and so
    // spacing/timestep still cannot separate a real 1 ps from a fabricated one.
    #[test]
    fn ddd_one_ps_over_a_one_fs_timestep_clears_every_check() {
        let toml = r#"
            lead_contributor_orcid = "0000-0000-0000-0000"
            trajectory_file_names = ["Pro_lig1.mdc"]
            structure_file_name = "Pro_lig.pdb"
            topology_file_name = "Pro_lig.gro"
            temperature_kelvin = 300
            integration_timestep_fs = 1
            sampling_frequency_ps = 1.0
            short_description = "A DDD dissociation trajectory ensemble"
            software_name = "SPONGE"
            software_version = "1.4"
        "#;
        let meta = Meta::from_toml(toml).expect("a DDD-shaped document parses");

        assert_eq!(meta.integration_timestep_fs, 1);
        assert_eq!(meta.sampling_frequency_ps, Some(1.0));
        assert!(
            !meta
                .check(None)
                .iter()
                .any(|m| m.starts_with("sampling_frequency_ps:")),
            "1 ps declared against a 1 fs timestep must validate"
        );

        assert_eq!(
            resolve_sampling_ps(
                1.0,
                meta.sampling_frequency_ps,
                Some(true),
                "Pro_lig1.mdc"
            )
            .unwrap(),
            1.0
        );

        // The steps-per-frame figure itself, computed the way
        // measure_trajectory computes it.
        let nstxout = 1.0 / (f64::from(meta.integration_timestep_fs) / FS_PER_PS);
        assert_eq!(nstxout, 1000.);
        assert!(
            nstxout <= 1e7,
            "the inflation guard must not fire on real DDD numbers"
        );
    }

    // Which marker a `.mdc` produces has not been established -- cpptraj
    // reports an AMBER trajectory as "is an AMBER trajectory", and the wrapper
    // matches on " is a " with a trailing space, so `unknown` is at least as
    // likely as `false`. Settling it needs a cpptraj run. Covering all three
    // arms means the remaining ~1,422 DDD imports behave identically whichever
    // it turns out to be.
    #[test]
    fn the_ddd_marker_is_not_settled_so_all_three_must_land_on_one_ps() {
        for axis in [Some(false), None, Some(true)] {
            assert_eq!(
                resolve_sampling_ps(1.0, Some(1.0), axis, "Pro_lig1.mdc").unwrap(),
                1.0,
                "marker {axis:?}"
            );
        }
    }

    // Below the floor the published figure is coarser than the rule that
    // admitted it. `sampling_frequency_ns` is rounded to 3 decimal places in
    // NANOSECONDS, so the whole 1-10 ps range collapses onto ten values: a
    // 6.25 ps spacing that survived the full corroboration ritual is published
    // as 6 ps, and 9.9 ps is published as though it sat exactly on the floor.
    // Meanwhile totaltime_ns is computed from the unrounded spacing, so the two
    // published fields disagree. The DDD's exact 1.0 ps is unaffected, which is
    // why this has never been noticed.
    #[test]
    fn a_sub_floor_spacing_is_published_at_one_ps_granularity() {
        for (spacing_ps, published_ns) in [
            (1.0, 0.001),
            (1.4, 0.001),
            (1.5, 0.002),
            (2.5, 0.003),
            (6.25, 0.006),
            (9.9, 0.01),
        ] {
            assert_eq!(
                round_dp(spacing_ps / PS_PER_NS, 3),
                published_ns,
                "{spacing_ps} ps"
            );
        }
    }

    // The marker is a bare string contract across two repositories, produced by
    // report_time_axis() in simulation-processing's
    // cpptraj_gmx_traj_manipulation.py and consumed here. Only the exact
    // literals `true` and `false` mean anything; everything else degrades to
    // None, which is the SAFE-LOOKING answer -- it restores the pre-2026-09-08
    // behaviour where a converter's fabricated 1 ps was measured back and
    // believed. A rename or a capitalisation change on the python side would
    // break this silently and no test anywhere would fail. `True` is not
    // paranoia: it is what `str(has_time)` produces.
    #[test]
    fn the_time_axis_marker_maps_only_two_literals() {
        const M: &str = "[mdrepo] source_has_time_axis=";

        for (value, want) in [
            ("true", Some(true)),
            ("false", Some(false)),
            ("unknown", None),
            ("True", None),
            ("TRUE", None),
            (" true", Some(true)),
            ("", None),
            ("yes", None),
            ("1", None),
        ] {
            assert_eq!(
                parse_time_axis_marker(&format!("{M}{value}")),
                want,
                "value {value:?}"
            );
        }

        assert_eq!(parse_time_axis_marker(""), None);
        assert_eq!(
            parse_time_axis_marker("Reading 'x.dcd' as Charmm DCD\n"),
            None
        );
    }

    // The marker need not be the last line, and it arrives indented and
    // sometimes CRLF-terminated out of a subprocess pipe. The .trim() that
    // makes that work is load-bearing and was unasserted.
    #[test]
    fn the_marker_is_found_in_real_wrapper_stdout() {
        let stdout = "Reading 'Pro_lig1.mdc' as Amber Trajectory\n\
                      \tINPUT TRAJECTORIES (1 total):\n\
                      \t[mdrepo] source_has_time_axis=false\r\n\
                      Running 2 threads\n\
                      TIME: Total execution time: 0.5000 seconds.\n";

        assert_eq!(parse_time_axis_marker(stdout), Some(false));
    }

    // The wrapper emits this marker from two call sites. If a code path ever
    // reaches both in one invocation the LAST one wins, so an `unknown` from a
    // second, unrelated cpptraj call would erase a genuine `false` from the
    // first and re-enable the fabricated spacing. Worth knowing before anyone
    // adds a third emission point.
    #[test]
    fn the_last_marker_wins_and_unknown_erases_a_finding() {
        const M: &str = "[mdrepo] source_has_time_axis=";

        assert_eq!(
            parse_time_axis_marker(&format!("{M}true\n{M}false\n")),
            Some(false)
        );
        assert_eq!(
            parse_time_axis_marker(&format!("{M}false\n{M}unknown\n")),
            None
        );
    }

    // strip_prefix anchors after the trim, so output that merely quotes the
    // marker -- an error message, or a `set -x` trace of the print that emits
    // it -- cannot spoof a finding.
    #[test]
    fn a_marker_quoted_inside_a_log_line_is_not_matched() {
        assert_eq!(
            parse_time_axis_marker(
                "note: [mdrepo] source_has_time_axis=false was expected"
            ),
            None
        );
        assert_eq!(
            parse_time_axis_marker("xx[mdrepo] source_has_time_axis=true"),
            None
        );
    }

    // The literal bytes of a duration.json written before the floor existed,
    // taken from ddd/work/2zc9/2zc9/processed/. Duration carries
    // deny_unknown_fields, so renaming a field would break the cached artifact
    // in 6,659 already-imported bundles -- and because get_duration reads the
    // cache before doing anything else, that failure surfaces on the next
    // reprocess rather than at the point of change.
    #[test]
    fn the_duration_json_already_on_disk_still_deserialises() {
        let cached = r#"{"totaltime_ns": 6.14, "sampling_frequency_ns": 0.001}"#;
        let duration: Duration =
            serde_json::from_str(cached).expect("the on-disk shape");

        assert_eq!(duration.totaltime_ns, 6.14);
        assert_eq!(duration.sampling_frequency_ns, 0.001_f32);

        // 0.001 ns per frame is the DDD's 1 ps, which is what every one of
        // those bundles recorded.
        assert_eq!(
            (f64::from(duration.sampling_frequency_ns) * PS_PER_NS).round(),
            1.0
        );
    }

    // The bug this function exists for. Shape taken from BioEmu ONE-cath1
    // cath1_1b43A02/run006_protein.cmprsd.xtc: 501 frames 10000 ps apart in
    // four concatenated segments, whose last one ends at 30000 ps. molly --info
    // reports `time: 0-30000 ps`, so the old span-based reading called this
    // 30000 ps -- 0.6% of the truth.
    #[test]
    fn modal_gap_survives_a_clock_that_restarts_mid_file() {
        let mut times = Vec::new();
        for len in [201, 7, 290, 3] {
            let first = if times.is_empty() { 0 } else { 1 };
            times.extend((first..first + len).map(|i| i as f64 * 10000.));
        }
        assert_eq!(times.len(), 501);
        assert_eq!(*times.last().unwrap(), 30000.);

        assert_eq!(modal_gap_ps(&times), Some(10000.));
        let duration_ps = (times.len() as f64 - 1.) * modal_gap_ps(&times).unwrap();
        assert_eq!(duration_ps, 5_000_000.);
        // What the span would have said, for the record.
        assert_eq!(times.last().unwrap() - times[0], 30000.);
    }

    // f32 stamps at multi-microsecond times jitter in the low bits. All of
    // these are the same 10000 ps spacing and must not split the mode.
    #[test]
    fn modal_gap_absorbs_f32_jitter_in_the_stamps() {
        let mut times = vec![0.];
        for gap in [10000.0, 9999.5, 10000.5, 9999.875, 10000.125, 10000.0] {
            times.push(times.last().unwrap() + gap);
        }
        let gap = modal_gap_ps(&times).expect("a spacing");
        assert!(
            (gap - 10000.).abs() < 1.,
            "jitter split the mode: got {gap}"
        );
    }

    // Rounding to 4 significant digits must not sort two all-but-identical gaps
    // into different buckets just because they straddle a power of ten.
    #[test]
    fn gap_bucket_agrees_across_a_power_of_ten() {
        assert_eq!(gap_bucket(9999.9999), gap_bucket(10000.0001));
        assert_eq!(gap_bucket(10000.), gap_bucket(10000.0001));
        // Genuinely different spacings still separate.
        assert_ne!(gap_bucket(10000.), gap_bucket(20000.));
        assert_ne!(gap_bucket(10000.), gap_bucket(100000.));
    }

    // A trajectory whose stamps never advance has no measurable spacing, and
    // saying so is better than reporting a zero-length run.
    #[test]
    fn modal_gap_is_none_when_no_frame_advances() {
        assert_eq!(modal_gap_ps(&[5., 5., 5.]), None);
        assert_eq!(modal_gap_ps(&[100., 50., 10.]), None);
        assert_eq!(modal_gap_ps(&[42.]), None);
    }

    #[test]
    fn frame_times_parse_from_molly_times_output() {
        // molly leaves a trailing tab where the --steps column would go.
        let stdout = "0.000\t\n10000.000\t\n20000.000\t\n";
        let times = parse_frame_times(stdout, Path::new("t.xtc")).unwrap();
        assert_eq!(times, vec![0., 10000., 20000.]);
    }

    #[test]
    fn frame_times_parse_tolerates_blank_lines_and_rejects_garbage() {
        let times =
            parse_frame_times("0.000\t\n\n10000.000\t\n", Path::new("t.xtc")).unwrap();
        assert_eq!(times, vec![0., 10000.]);
        assert!(parse_frame_times("0.000\tnot-a-time\n", Path::new("t.xtc")).is_ok());
        assert!(parse_frame_times("not-a-time\n", Path::new("t.xtc")).is_err());
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
            blast_uniprot(&fasta, blast_dir.path(), UniprotDb::Swissprot, 2).unwrap();
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
        let ids =
            blast_uniprot(&fasta, blast_dir.path(), UniprotDb::Trembl, 2).unwrap();
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
            blast_uniprot(&fasta, blast_dir.path(), UniprotDb::Swissprot, 2).unwrap();
        assert_eq!(ids, vec!["P99999".to_string()]);
    }

    #[test]
    fn blast_uniprot_rejects_nonexistent_blast_dir() {
        let tmp = tempdir().unwrap();
        let fasta = tmp.path().join("sequence.fa");
        fs::write(&fasta, b">1\nACGT").unwrap();
        assert!(
            blast_uniprot(
                &fasta,
                Path::new("/nonexistent/blast"),
                UniprotDb::Swissprot,
                2
            )
            .is_err()
        );
    }
}
