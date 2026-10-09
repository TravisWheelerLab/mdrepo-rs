//! The chain backfill: md_chain, md_polymer and each polymer's UniProt
//! reference for simulations imported before mdr-process wrote them (0.4.0).
//!
//! Input is a TSV (`simulation_id`, `pdb_id`, `structure`): each
//! simulation's ID, its declared PDB ID (may be blank) and a local copy of
//! its processed structure. The chains come from the same splitter imports
//! use (`sequence.rs`), so they carry their backbone breaks. Only `structure`
//! chains: a ligand declared by sequence (a `declared` chain) needs the
//! TOML, which this does not read.
//!
//! The lookups run as one batch (`reference::lookup_polymers`): the PDB index
//! read once, one BLAST over every polymer that needs it, each UniProt entry
//! fetched once. A polymer shared by several simulations is decided by the
//! lowest simulation ID among them, as if the simulations had been imported
//! in ID order with mdr-process 0.6.0 (the first import decides); the others
//! are counted in the report when their own PDB entry would have given
//! another accession.
//!
//! Writes: per simulation, one transaction (`import::backfill_chains`). A
//! simulation that already has chains is left alone, and a polymer that
//! already has a reference keeps it. `--dry-run` reads no database and
//! writes none; the report is the same.

use crate::{
    import,
    reference::{self, PolymerQuery},
    sequence,
    ticket::dsn_for,
    types::{BackfillArgs, ImportChain, PolymerLookup},
};
use anyhow::{Result, anyhow, bail};
use log::{debug, info, warn};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

/// One simulation of the input
#[derive(Debug, Clone)]
struct Row {
    simulation_id: i64,
    pdb_id: Option<String>,
    structure: PathBuf,
}

// --------------------------------------------------
pub fn backfill_chains(args: &BackfillArgs) -> Result<()> {
    let blast_dir = match &args.blast_dir {
        Some(dir) => dir.clone(),
        None => PathBuf::from(
            env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
        )
        .join("blast"),
    };
    fs::create_dir_all(&args.work_dir)
        .map_err(|e| anyhow!("{}: {e}", args.work_dir.display()))?;

    let mut rows = read_input(&args.input)?;
    rows.sort_by_key(|r| r.simulation_id);
    info!("{} simulations", rows.len());

    // Split every structure
    let split: Vec<Result<Vec<ImportChain>>> =
        rows.par_iter().map(|r| chains_of(&r.structure)).collect();

    // Distinct polymers in simulation ID order; the first simulation with a
    // polymer decides it
    let mut keys: HashMap<(String, Vec<String>), usize> = HashMap::new();
    let mut queries: Vec<PolymerQuery> = vec![];
    let mut decided_by: Vec<i64> = vec![];
    // Every (simulation, PDB ID) that has each polymer, for the report
    let mut sharing: Vec<Vec<(i64, Option<String>)>> = vec![];
    for (row, chains) in rows.iter().zip(&split) {
        let Ok(chains) = chains else { continue };
        for chain in chains {
            let key = (chain.polymer_type.clone(), chain.residues.clone());
            let i = *keys.entry(key).or_insert_with(|| {
                queries.push(PolymerQuery {
                    polymer_type: chain.polymer_type.clone(),
                    num_residues: chain.residues.len(),
                    sequence: chain.sequence.clone(),
                    breaks: chain.breaks.clone(),
                    pdb_id: row.pdb_id.clone(),
                });
                decided_by.push(row.simulation_id);
                sharing.push(vec![]);
                queries.len() - 1
            });
            if !sharing[i].iter().any(|(s, _)| *s == row.simulation_id) {
                sharing[i].push((row.simulation_id, row.pdb_id.clone()));
            }
        }
    }
    info!("{} distinct polymers", queries.len());

    let (lookups, warnings) = reference::lookup_polymers(
        &queries,
        &blast_dir,
        &args.work_dir,
        args.blast_num_threads,
    )?;
    for warning in &warnings {
        warn!("{warning}");
    }
    let disagreeing = disagreements(&queries, &lookups, &sharing, &blast_dir)?;

    // Write, simulation by simulation, in ID order
    let mut conn = match (&args.server, args.dry_run) {
        (_, true) => None,
        (Some(server), false) => {
            let key = dsn_for(server);
            let url = env::var(key).map_err(|e| anyhow!("{key}: {e}"))?;
            Some(mdr_db::connect(&url).map_err(|e| anyhow!("Failed to connect: {e}"))?)
        }
        (None, false) => bail!("--server is required unless --dry-run"),
    };

    let mut out =
        File::create(&args.out).map_err(|e| anyhow!("{}: {e}", args.out.display()))?;
    writeln!(
        out,
        "simulation_id\tpdb_id\tstatus\tchain_order\tchain_label\tpolymer_type\t\
         num_residues\tbreaks\tmatch_method\taccession\tquery_start\tquery_end\t\
         reference_start\treference_end\tidentity\tdecided_by\tdisagreeing_sims"
    )?;
    let (mut written, mut skipped, mut failed) = (0, 0, 0);
    for (row, chains) in rows.iter().zip(split) {
        let pdb = row.pdb_id.as_deref().unwrap_or("");
        let mut chains = match chains {
            Ok(chains) => chains,
            Err(e) => {
                failed += 1;
                writeln!(out, "{}\t{pdb}\tunreadable: {e}", row.simulation_id)?;
                continue;
            }
        };
        let mut polymer_of = vec![];
        for chain in chains.iter_mut() {
            let i = keys[&(chain.polymer_type.clone(), chain.residues.clone())];
            chain.reference = lookups[i].clone();
            polymer_of.push(i);
        }

        let status = match conn.as_mut() {
            None => "dry run".to_string(),
            Some(conn) => {
                match import::backfill_chains(conn, row.simulation_id, &chains) {
                    Ok(true) => {
                        written += 1;
                        "written".to_string()
                    }
                    Ok(false) => {
                        skipped += 1;
                        "has chains; left alone".to_string()
                    }
                    Err(e) => {
                        failed += 1;
                        format!("failed: {e}")
                    }
                }
            }
        };
        debug!("{}: {status}", row.simulation_id);

        for (chain, &i) in chains.iter().zip(&polymer_of) {
            writeln!(
                out,
                "{}\t{pdb}\t{status}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.simulation_id,
                chain.chain_order,
                chain.chain_label,
                chain.polymer_type,
                chain.residues.len(),
                chain.breaks.len(),
                report_lookup(chain.reference.as_ref()),
                decided_by[i],
                disagreeing[i],
            )?;
        }
    }
    info!(
        "{} written, {} left alone, {} failed (report: {})",
        written,
        skipped,
        failed,
        args.out.display()
    );
    Ok(())
}

/// The input TSV: a header, then `simulation_id`, `pdb_id`, `structure`
fn read_input(path: &Path) -> Result<Vec<Row>> {
    let text =
        fs::read_to_string(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let mut rows = vec![];
    for (n, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            bail!("{} line {}: want 3 columns", path.display(), n + 1);
        }
        let id = fields[0].trim().trim_start_matches("MDR");
        rows.push(Row {
            simulation_id: id
                .parse()
                .map_err(|e| anyhow!("{} line {}: {e}", path.display(), n + 1))?,
            pdb_id: Some(fields[1].trim().to_lowercase()).filter(|p| !p.is_empty()),
            structure: PathBuf::from(fields[2].trim()),
        });
    }
    Ok(rows)
}

/// A structure's polymer chains as the import writes them
fn chains_of(structure: &Path) -> Result<Vec<ImportChain>> {
    let chains = sequence::chains_from_pdb(structure)?;
    Ok(chains
        .into_iter()
        .enumerate()
        .map(|(i, c)| ImportChain {
            chain_order: i as u32 + 1,
            chain_label: c.label,
            source: "structure".to_string(),
            polymer_type: c.polymer_type.to_string(),
            sequence: c.sequence,
            residues: c.residues,
            first_residue: Some(c.first_residue),
            last_residue: Some(c.last_residue),
            n_terminal_cap: c.n_terminal_cap,
            c_terminal_cap: c.c_terminal_cap,
            breaks: c.breaks,
            reference: None,
        })
        .collect())
}

/// For each polymer, how many of the other simulations sharing it would
/// have matched their own declared entry to a different accession than the
/// one decided. Compared without fetching UniProt.
fn disagreements(
    queries: &[PolymerQuery],
    lookups: &[Option<PolymerLookup>],
    sharing: &[Vec<(i64, Option<String>)>],
    blast_dir: &Path,
) -> Result<Vec<usize>> {
    let shared: Vec<usize> = (0..queries.len())
        .filter(|&i| sharing[i].len() > 1 && queries[i].polymer_type == "protein")
        .collect();
    let mut counts = vec![0; queries.len()];
    if shared.is_empty() {
        return Ok(counts);
    }
    let seqres = blast_dir.join(reference::PDB_SEQRES);
    let sifts = blast_dir.join(reference::PDB_SIFTS);
    if !(seqres.is_file() && sifts.is_file()) {
        return Ok(counts);
    }
    let pdb_ids = shared
        .iter()
        .flat_map(|&i| sharing[i].iter().filter_map(|(_, p)| p.clone()))
        .collect();
    let entries = reference::entries_chains(&seqres, &pdb_ids)?;
    let pairs = entries
        .iter()
        .flat_map(|(pdb, chains)| chains.iter().map(|(c, _)| (pdb.clone(), c.clone())))
        .collect();
    let segments = reference::sifts_segments(&sifts, &pairs)?;
    for &i in &shared {
        let decided = lookups[i]
            .as_ref()
            .and_then(|l| l.hit.as_ref())
            .map(|h| h.uniprot.uniprot_id.as_str());
        let q = &queries[i];
        counts[i] = sharing[i]
            .iter()
            .skip(1)
            .filter(|(_, pdb)| {
                let own = pdb.as_ref().and_then(|pdb| {
                    let chains = entries.get(pdb)?;
                    reference::pdb_candidate(
                        &q.sequence,
                        &q.breaks,
                        pdb,
                        chains,
                        &segments,
                    )
                });
                // Only a simulation whose own entry matches can disagree
                own.is_some_and(|c| Some(c.accession.as_str()) != decided)
            })
            .count();
    }
    Ok(counts)
}

/// match_method, accession, query and reference ranges, identity
fn report_lookup(lookup: Option<&PolymerLookup>) -> String {
    match lookup {
        None => "untried\t\t\t\t\t\t".to_string(),
        Some(PolymerLookup {
            match_method,
            hit: None,
        }) => format!("{match_method}\t\t\t\t\t\t"),
        Some(PolymerLookup {
            match_method,
            hit: Some(h),
        }) => format!(
            "{match_method}\t{}\t{}\t{}\t{}\t{}\t{:.1}",
            h.uniprot.uniprot_id,
            h.query_start,
            h.query_end,
            h.reference_start,
            h.reference_end,
            h.identity
        ),
    }
}
