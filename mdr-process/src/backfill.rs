//! The chain backfill: md_chain, md_polymer and each polymer's UniProt
//! reference for simulations imported before mdr-process wrote them (0.4.0).
//!
//! Input is a TSV (`simulation_id`, `pdb_id`, `structure`, and optionally
//! `uniprot_ids`): each simulation's ID, its declared PDB ID (may be blank),
//! a local copy of its processed structure, and its declared UniProt IDs
//! (comma-separated, may be blank; md_simulation_uniprot). The chains come from the same splitter imports
//! use (`sequence.rs`), so they carry their backbone breaks. Only `structure`
//! chains: a ligand declared by sequence (a `declared` chain) needs the
//! TOML, which this does not read.
//!
//! The lookups run as one batch (`reference::lookup_polymers`): SIFTS read
//! once, one search over every polymer that needs it, each UniProt entry
//! fetched once. A polymer shared by several simulations is decided by the
//! lowest simulation ID among them, as if the simulations had been imported
//! in ID order (the first import decides); the others are counted in the
//! report when their own declared candidates would have given another
//! accession.
//!
//! UniProt entries and UniRef100 representatives are cached in
//! `<work_dir>/uniprot/`, so a later run over
//! the same work folder (the real run after its dry run) reuses them and
//! writes what the dry run reported.
//!
//! Writes: per simulation, one transaction (`import::backfill_chains`). A
//! polymer that already has a reference keeps it. A simulation that already
//! has chains keeps them, and only its polymers never looked up get a
//! reference (the DDD peptides, imported on 0.4.0 with chains and no
//! references; Ken, 2026-10-14). With `--replace-chains`, a simulation whose
//! stored structure chains differ from the structure file's has them
//! replaced (`import::replace_structure_chains`; the DDD peptides' chains,
//! written before their files were relabeled). `--dry-run` reads no database and
//! writes none; the report is the same.

use crate::{
    import::{self, Backfilled},
    reference::{self, Declared, PolymerQuery},
    sequence,
    ticket::dsn_for,
    types::{BackfillArgs, ImportChain, PolymerLookup},
};
use anyhow::{Result, anyhow, bail};
use log::{debug, info, warn};
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
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
    uniprot_ids: Vec<String>,
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
    // Every simulation that has each polymer, for the report
    let mut sharing: Vec<Vec<&Row>> = vec![];
    for (row, chains) in rows.iter().zip(&split) {
        let Ok(chains) = chains else { continue };
        for chain in chains {
            let key = (chain.polymer_type.clone(), chain.residues.clone());
            let i = *keys.entry(key).or_insert_with(|| {
                queries.push(PolymerQuery {
                    polymer_type: chain.polymer_type.clone(),
                    num_residues: chain.residues.len(),
                    sequence: chain.sequence.clone(),
                    pdb_id: row.pdb_id.clone(),
                    uniprot_ids: row.uniprot_ids.clone(),
                });
                decided_by.push(row.simulation_id);
                sharing.push(vec![]);
                queries.len() - 1
            });
            if !sharing[i]
                .iter()
                .any(|s| s.simulation_id == row.simulation_id)
            {
                sharing[i].push(row);
            }
        }
    }
    info!("{} distinct polymers", queries.len());

    let cache = args.work_dir.join("uniprot");
    let (lookups, warnings, related) = reference::lookup_polymers(
        &queries,
        &blast_dir,
        &args.work_dir,
        args.blast_num_threads,
        Some(&cache),
    )?;
    for warning in &warnings {
        warn!("{warning}");
    }
    let (disagreeing, own) = disagreements(
        &queries,
        &lookups,
        &sharing,
        &blast_dir,
        &args.work_dir.join("disagreements"),
        &cache,
    )?;

    // The UniProt entries SIFTS lists for each declared PDB ID, read once
    let sifts = blast_dir.join(reference::PDB_SIFTS);
    let listed = if sifts.is_file() {
        let pdb_ids: HashSet<String> = rows
            .iter()
            .filter_map(|r| r.pdb_id.as_ref().map(|p| p.to_lowercase()))
            .collect();
        reference::sifts_accessions(&sifts, &pdb_ids)?
    } else {
        HashMap::new()
    };

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
         reference_start\treference_end\tidentity\tdecided_by\tdisagreeing_sims\t\
         pdb_id_check"
    )?;
    let (mut written, mut had_chains, mut references_set, mut replaced, mut failed) =
        (0, 0, 0, 0, 0);
    let mut pdb_mismatches = 0;
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
        let pdb_check = pdb_check(
            row,
            &polymer_of,
            &queries,
            &lookups,
            &related,
            &decided_by,
            &own,
            &listed,
        );
        if let Some(warning) = &pdb_check {
            pdb_mismatches += 1;
            warn!("{}: {warning}", row.simulation_id);
        }

        let status = match conn.as_mut() {
            None => "dry run".to_string(),
            Some(conn) => {
                let result = if args.replace_chains {
                    import::replace_structure_chains(conn, row.simulation_id, &chains)
                } else {
                    import::backfill_chains(conn, row.simulation_id, &chains)
                };
                match result {
                    Ok(Backfilled::Written) => {
                        written += 1;
                        "written".to_string()
                    }
                    Ok(Backfilled::ReferencesSet(n)) => {
                        had_chains += 1;
                        references_set += n;
                        format!("has chains; {n} references set")
                    }
                    Ok(Backfilled::Replaced) => {
                        replaced += 1;
                        "chains replaced".to_string()
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
                "{}\t{pdb}\t{status}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.simulation_id,
                chain.chain_order,
                chain.chain_label,
                chain.polymer_type,
                chain.residues.len(),
                chain.breaks.len(),
                report_lookup(chain.reference.as_ref()),
                decided_by[i],
                disagreeing[i],
                if pdb_check.is_some() { "no match" } else { "" },
            )?;
        }
    }
    info!(
        "{} written, {} chains replaced, {} had chains ({} untried polymers \
         given a reference), {} failed, {} with a PDB ID no chain matches \
         (report: {})",
        written,
        replaced,
        had_chains,
        references_set,
        failed,
        pdb_mismatches,
        args.out.display()
    );
    Ok(())
}

/// The input TSV: a header, then `simulation_id`, `pdb_id`, `structure`,
/// and optionally `uniprot_ids` (comma-separated)
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
            uniprot_ids: fields
                .get(3)
                .map(|ids| {
                    ids.split(',')
                        .map(|id| id.trim().to_uppercase())
                        .filter(|id| !id.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
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

/// Whether each other simulation's own declared PDB entry is related to a
/// polymer it shares (`reference::related_to_pdb`): (polymer, simulation
/// ID) -> related, `None` when its step 2 could not be finished
type OwnMatches = HashMap<(usize, i64), Option<bool>>;

/// For each polymer, how many of the other simulations sharing it would
/// have matched their own declared candidates (step 2) to a different
/// accession than the one decided; and, for the PDB ID check, whether each
/// of those simulations' own PDB entry is related to the polymer. Their
/// BLAST files go in `work_dir`.
fn disagreements(
    queries: &[PolymerQuery],
    lookups: &[Option<PolymerLookup>],
    sharing: &[Vec<&Row>],
    blast_dir: &Path,
    work_dir: &Path,
    cache: &Path,
) -> Result<(Vec<usize>, OwnMatches)> {
    let mut counts = vec![0; queries.len()];
    let mut own = HashMap::new();
    let sifts = blast_dir.join(reference::PDB_SIFTS);
    if !sifts.is_file() {
        return Ok((counts, own));
    }

    // One query per polymer and other simulation that declares something
    let mut others: Vec<PolymerQuery> = vec![];
    let mut polymer_of: Vec<(usize, i64)> = vec![];
    for (i, q) in queries.iter().enumerate() {
        if q.polymer_type != "protein" || q.num_residues < reference::MIN_RESIDUES {
            continue;
        }
        for row in sharing[i].iter().skip(1) {
            if row.pdb_id.is_none() && row.uniprot_ids.is_empty() {
                continue;
            }
            others.push(PolymerQuery {
                pdb_id: row.pdb_id.clone(),
                uniprot_ids: row.uniprot_ids.clone(),
                ..q.clone()
            });
            polymer_of.push((i, row.simulation_id));
        }
    }
    if others.is_empty() {
        return Ok((counts, own));
    }

    let wanted: Vec<usize> = (0..others.len()).collect();
    let (declared, _, _, related) =
        reference::declared_matches(&others, &wanted, &sifts, work_dir, Some(cache))?;
    for (j, &(i, sim)) in polymer_of.iter().enumerate() {
        let decided = lookups[i]
            .as_ref()
            .and_then(|l| l.hit.as_ref())
            .map(|h| h.uniprot.uniprot_id.as_str());
        // Only a simulation whose own candidates match can disagree
        if let Some(Declared::Match(c)) = declared.get(&j)
            && Some(c.accession.as_str()) != decided
        {
            counts[i] += 1;
        }
        if let Some(outcome) = declared.get(&j) {
            let known = !matches!(outcome, Declared::Unfinished);
            own.insert((i, sim), known.then(|| related.contains(&j)));
        }
    }
    Ok((counts, own))
}

/// The PDB ID check (`reference::declared_pdb_not_matched`) for one
/// simulation, from its own declared IDs: a polymer it decided, by its
/// lookup (`related`, from `lookup_polymers`); one another simulation
/// decided, by its own step 2 (`own`)
#[allow(clippy::too_many_arguments)]
fn pdb_check(
    row: &Row,
    polymers: &[usize],
    queries: &[PolymerQuery],
    lookups: &[Option<PolymerLookup>],
    related: &HashSet<usize>,
    decided_by: &[i64],
    own: &OwnMatches,
    listed: &HashMap<String, Vec<String>>,
) -> Option<String> {
    let pdb_id = row.pdb_id.as_deref()?;
    let listed = listed.get(&pdb_id.to_lowercase())?;
    let mut seen = vec![];
    let flags: Vec<Option<bool>> = polymers
        .iter()
        .filter(|&&i| {
            let q = &queries[i];
            let new = !seen.contains(&i);
            seen.push(i);
            new && q.polymer_type == "protein"
                && q.num_residues >= reference::MIN_RESIDUES
        })
        .map(|&i| {
            if decided_by[i] == row.simulation_id {
                lookups[i].as_ref().map(|_| related.contains(&i))
            } else {
                own.get(&(i, row.simulation_id)).copied().flatten()
            }
        })
        .collect();
    reference::declared_pdb_not_matched(pdb_id, listed, &flags)
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

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::{NONE, PDB};

    fn row(id: i64, pdb: &str) -> Row {
        Row {
            simulation_id: id,
            pdb_id: Some(pdb.into()),
            structure: PathBuf::new(),
            uniprot_ids: vec![],
        }
    }

    fn protein(n: usize) -> PolymerQuery {
        PolymerQuery {
            polymer_type: "protein".into(),
            num_residues: n,
            sequence: "A".repeat(n),
            pdb_id: None,
            uniprot_ids: vec![],
        }
    }

    fn lookup(method: &str) -> Option<PolymerLookup> {
        Some(PolymerLookup {
            match_method: method.into(),
            hit: None,
        })
    }

    /// A polymer the simulation decided is judged by its own lookup; one
    /// another simulation decided, by the simulation's own step 2, so a
    /// reference matched through someone else's PDB ID does not hide a
    /// wrong one (84136 shares its polymer with 11551, 3wk5)
    #[test]
    fn pdb_check_uses_each_simulations_own_lookup() {
        // Polymer 0: decided by 11551, related to its 3wk5. Polymer 1: a
        // peptide, too short to count. Polymer 2: decided by 84136, not
        // looked up.
        let queries = vec![protein(546), protein(9), protein(100)];
        let lookups = vec![lookup(PDB), lookup(NONE), None];
        let related = HashSet::from([0]);
        let decided_by = vec![11551, 84136, 84136];
        let listed: HashMap<String, Vec<String>> = [
            ("4tq8".to_string(), vec!["P02766".to_string()]),
            ("3wk5".to_string(), vec!["P34913".to_string()]),
        ]
        .into();
        let check = |sim: &Row, polymers: &[usize], own: &OwnMatches| {
            pdb_check(
                sim,
                polymers,
                &queries,
                &lookups,
                &related,
                &decided_by,
                own,
                &listed,
            )
        };

        // 84136's own 4tq8 is not related: warned, though the polymer's
        // decider was related to its own entry
        let own: OwnMatches = [((0, 84136), Some(false))].into();
        let got = check(&row(84136, "4tq8"), &[0, 0, 1], &own).unwrap();
        assert!(got.contains("4tq8"), "{got}");

        // Related by its own step 2: no warning (PDB IDs in any case)
        let own: OwnMatches = [((0, 84136), Some(true))].into();
        assert_eq!(check(&row(84136, "4TQ8"), &[0, 1], &own), None);

        // Not known: its step 2 unfinished or absent, or a polymer it
        // decided left untried
        let own: OwnMatches = [((0, 84136), None)].into();
        assert_eq!(check(&row(84136, "4tq8"), &[0, 1], &own), None);
        assert_eq!(check(&row(84136, "4tq8"), &[0, 1], &HashMap::new()), None);
        let own: OwnMatches = [((0, 84136), Some(false))].into();
        assert_eq!(check(&row(84136, "4tq8"), &[0, 2], &own), None);

        // The decider is judged by its lookup
        assert_eq!(check(&row(11551, "3wk5"), &[0], &HashMap::new()), None);

        // A PDB ID SIFTS does not list, or none: no check
        assert_eq!(check(&row(84136, "9zzz"), &[0], &own), None);
        let no_pdb = Row {
            pdb_id: None,
            ..row(84136, "")
        };
        assert_eq!(check(&no_pdb, &[0], &own), None);
    }
}
