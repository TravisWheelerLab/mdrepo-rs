use crate::{
    common::{file_exists, get_md5},
    metadata::{Meta, MoleculeType},
    types::{
        Duration, PdbEntry, PdbGraphqlResponse, PdbResponse, PdbUniprot, ProcessArgs,
        ProcessedFiles, ProteinSequence, RmsdRmsf, Server, UniprotEntry,
        UniprotResponse,
    },
};
use anyhow::{anyhow, bail, Result};
use dotenvy::dotenv;
use log::info;
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::Command,
};
use which::which;

// --------------------------------------------------
pub fn process(args: &ProcessArgs) -> Result<()> {
    dbg!(&args);
    let input_dir = &args.dirname;
    let processed_dir = args
        .outdir
        .clone()
        .map_or(input_dir.join("processed"), |dir| PathBuf::from(&dir));
    let script_dir = &args.script_dir.clone().unwrap();
    dbg!(&processed_dir);

    let meta_path = input_dir.join("mdrepo-metadata.toml");
    let meta = Meta::from_file(&meta_path)?;
    let processed_files =
        make_processed_files(&meta, &input_dir, &processed_dir, &script_dir)?;

    //let json_dir = &args.json_dir.clone().unwrap();
    //let in_dir_basename = &in_dir.file_name().unwrap().to_string_lossy().to_string();
    //let import_json = json_dir.join(format!("{in_dir_basename}.json"));
    import(
        &meta,
        &input_dir,
        &script_dir,
        &processed_files,
        &args.server,
    )?;

    Ok(())
}

// --------------------------------------------------
fn make_thumbnail(
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
fn make_processed_files(
    meta: &Meta,
    in_dir: &PathBuf,
    processed_dir: &PathBuf,
    script_dir: &PathBuf,
) -> Result<ProcessedFiles> {
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
                &processed_dir.to_string_lossy().to_string(),
            ])
            .output()?;
        dbg!(&cmd);

        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
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
fn get_rmsd_rmsf(
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
fn get_sequence(full_pdb: &PathBuf, script_dir: &PathBuf) -> Result<ProteinSequence> {
    let processed_dir = full_pdb.parent().unwrap();
    let sequence_file = processed_dir.join("sequence.json");

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

    let contents = fs::read_to_string(&sequence_file)?;
    let sequence: ProteinSequence = serde_json::from_str(&contents)?;

    Ok(sequence)
}

// --------------------------------------------------
fn sample_trajectory(
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
pub fn db_connection(server: &Server) -> Result<postgres::Client> {
    dotenv().ok();

    let env_key = match server {
        Server::Production => "PRODUCTION_DB_URL",
        Server::Staging => "STAGING_DB_URL",
    };
    dbg!(&env_key);

    let db_url = env::var(env_key).expect(&format!("{env_key} must be set"));
    postgres::Client::connect(&db_url, postgres::NoTls)
        .map_err(|_| anyhow!("Failed db connection"))
}

// --------------------------------------------------
fn import(
    meta: &Meta,
    input_dir: &PathBuf,
    script_dir: &PathBuf,
    processed_files: &ProcessedFiles,
    server: &Server,
) -> Result<()> {
    let mut dbh = db_connection(server)?;
    println!("Connected to {server:?}");

    let topology = input_dir.join(&meta.required_files.topology_file_name);
    let topology_hash = get_topology_hash(&topology)?;
    //dbg!(&topology_hash);

    let sequence = get_sequence(&processed_files.full_pdb, &script_dir)?;
    dbg!(&sequence);

    let rmsd_rmsf = get_rmsd_rmsf(
        &processed_files.min_pdb,
        &processed_files.min_xtc,
        &script_dir,
    )?;
    //dbg!(&rmsd_rmsf);

    let duration = get_duration(&processed_files.full_xtc)?;
    //dbg!(&duration);

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
    let input_paths: Vec<PathBuf> =
        input_files.iter().map(|f| input_dir.join(f)).collect();

    let mut md5s: Vec<String> = input_paths
        .iter()
        .filter_map(|path| get_md5(path).ok())
        .collect();
    md5s.sort();
    let unique_file_hash = md5s.join(",");
    dbg!(&unique_file_hash);

    let sim_id = find_or_create_simulation(
        &mut dbh,
        &unique_file_hash,
        &meta,
        &sequence,
        &topology_hash,
        &duration,
        &rmsd_rmsf,
    )?;
    dbg!(&sim_id);

    //dbg!(&meta.proteins);
    //let mut uniprots: Vec<UniprotEntry> = vec![];
    //let mut pdbs: Vec<PdbEntry> = vec![];
    //for protein in &meta.proteins {
    //    match (
    //        protein.molecule_id_type.clone(),
    //        protein.molecule_id.clone(),
    //    ) {
    //        (Some(MoleculeType::Uniprot), Some(uniprot_id)) => {
    //            let uniprot = get_uniprot_entry(&uniprot_id)?;
    //            uniprots.push(uniprot);
    //        }
    //        (Some(MoleculeType::PDB), Some(pdb_id)) => {
    //            let pdb = get_pdb_entry(&pdb_id)?;
    //            pdbs.push(pdb);
    //        }
    //        _ => println!("Handle {protein:?}"),
    //    }
    //}
    //dbg!(&uniprots);
    //dbg!(&pdbs);

    Ok(())
}

// --------------------------------------------------
fn get_duration(full_xtc: &PathBuf) -> Result<Duration> {
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
            let duration = Duration {
                totaltime_ns: totaltime_ns as u32,
                sampling_frequency_ns,
            };
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

    Ok(PdbEntry {
        pdb_id: pdb_id.to_string(),
        title: pdb_resp.struct_.title.to_string(),
        classification: pdb_resp.struct_keywords.pdbx_keywords.to_string(),
        uniprots,
    })
}

// --------------------------------------------------
fn find_or_create_simulation(
    dbh: &mut postgres::Client,
    unique_file_hash: &str,
    _meta: &Meta,
    _sequence: &ProteinSequence,
    topology_hash: &str,
    _duration: &Duration,
    _rmsd_rmsf: &RmsdRmsf,
) -> Result<u64> {
    let replicate_group_id = find_or_create_replicate_group(dbh, topology_hash)?;
    dbg!(&replicate_group_id);

    let res = dbh.query(
        "select id from md_simulation where unique_file_hash_string=$1",
        &[&unique_file_hash],
    )?;

    if res.is_empty() {}
    dbg!(&res);
    Ok(0)
}

// --------------------------------------------------
fn find_or_create_replicate_group(
    dbh: &mut postgres::Client,
    topology_hash: &str,
) -> Result<i64> {
    let res = dbh.query(
        "select id from md_simulation_replicate_group where psf_hash=$1",
        &[&topology_hash],
    )?;

    if let Some(first) = res.first() {
        return Ok(first.get::<usize, i64>(0));
    }

    let res = dbh.query(
        "
        insert
        into   md_simulation_replicate_group (psf_hash)
        values ($1)
        returning id;
        ",
        &[&topology_hash],
    )?;

    if let Some(first) = res.first() {
        return Ok(first.get::<usize, i64>(0));
    }

    Ok(0)
}
