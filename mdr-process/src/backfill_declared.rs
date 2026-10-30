//! The declared UniProt backfill (`backfill-declared-uniprot`, 0.21.0): move
//! md_simulation_uniprot's links onto the chains before md-repo-app drops
//! the table (Ken, 2026-10-30). A link was written for every declared
//! UniProt ID, unchecked (or the top Swiss-Prot hit when none was
//! declared); a polymer's references (md_polymer_uniprot) are now the only
//! place a simulation's UniProt IDs are kept, and a declared one only when
//! it passes against a chain (Travis, 2026-10-22, MDR-104).
//!
//! Input is a TSV with a header, built by
//! `mdr-process/sql/backfill-declared-uniprot-candidates.sql`: each link
//! whose entry is on none of its simulation's chains, once per protein
//! polymer of the simulation (`simulation_id`, `accession`, `polymer_id`,
//! `num_residues`, `sequence`), the polymer columns blank when it has none.
//!
//! A chain too short for BLAST (5 to 19 residues) passes for each linked
//! entry that contains its sequence exactly (`reference::peptide_lookup`).
//! Each longer polymer runs step 2 alone (`reference::declared_matches`) with every
//! accession its simulations' links name as declared candidates, and no PDB
//! ID: each accession is compared with the polymer by `blastp -subject`,
//! and passes by the import's pass rule (`reference::passes`). A link
//! passes when its accession passes against any of its simulation's
//! polymers. The real run adds each accession that passes to the polymer
//! as a `declared` row, as an import now does
//! (`import::add_declared_references`); the polymer's reference is kept.
//! Nothing is deleted: md-repo-app drops md_simulation_uniprot, after
//! Travis has had the report (MDR-104).
//!
//! The report, `--out`, has a row per link: `passes` (with the polymers),
//! `fails` (with the closest BLAST row, or none), `no_protein_chain`, `gone`
//! (UniProt will not serve the entry: deleted, demerged, not an accession)
//! or `unfinished` (a fetch that failed, for a later run).
//!
//! UniProt entries are cached in `<work_dir>/uniprot/` and BLAST's files go
//! in `<work_dir>/declared/`: give the real run its dry run's work dir and
//! it compares the same entries.

use crate::{
    import,
    reference::{
        self, Candidate, Declared, DeclaredMatches, PolymerBlastHit, PolymerQuery,
    },
    ticket::dsn_for,
    types::{BackfillDeclaredUniprotArgs, PolymerHit, PolymerLookup},
};
use anyhow::{Result, anyhow, bail};
use diesel::connection::Connection;
use log::{info, warn};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env,
    fs::{self, File},
    io::Write,
    path::Path,
};

/// One line of the input
#[derive(Debug, Clone, PartialEq)]
struct Row {
    simulation_id: i64,
    accession: String,
    polymer: Option<Polymer>,
}

#[derive(Debug, Clone, PartialEq)]
struct Polymer {
    id: i64,
    num_residues: usize,
    sequence: String,
}

/// What came of a link
const PASSES: &str = "passes";
const FAILS: &str = "fails";
const NO_PROTEIN_CHAIN: &str = "no_protein_chain";
const GONE: &str = "gone";
const UNFINISHED: &str = "unfinished";

// --------------------------------------------------
pub fn backfill_declared_uniprot(args: &BackfillDeclaredUniprotArgs) -> Result<()> {
    if !args.dry_run && args.server.is_none() {
        bail!("--server is required unless --dry-run");
    }
    fs::create_dir_all(&args.work_dir)
        .map_err(|e| anyhow!("{}: {e}", args.work_dir.display()))?;
    let rows = read_input(&args.input)?;

    // A query per polymer, its candidates every accession linked to a
    // simulation that has it
    let mut polymers: BTreeMap<i64, (&Polymer, BTreeSet<String>)> = BTreeMap::new();
    for row in &rows {
        if let Some(p) = &row.polymer {
            polymers
                .entry(p.id)
                .or_insert_with(|| (p, BTreeSet::new()))
                .1
                .insert(row.accession.to_uppercase());
        }
    }
    let ids: Vec<i64> = polymers.keys().copied().collect();
    let queries: Vec<PolymerQuery> = polymers
        .values()
        .map(|(p, accs)| PolymerQuery {
            polymer_type: "protein".to_string(),
            num_residues: p.num_residues,
            sequence: p.sequence.clone(),
            pdb_ids: vec![],
            uniprot_ids: accs.iter().cloned().collect(),
        })
        .collect();
    let num_links = rows
        .iter()
        .map(|r| (r.simulation_id, &r.accession))
        .collect::<BTreeSet<_>>()
        .len();
    info!("{num_links} links, {} polymers", queries.len());

    // No PDB IDs, so SIFTS is never read. A chain too short for BLAST is
    // looked for in its declared entries (`reference::peptide_lookup`).
    let (wanted, short): (Vec<usize>, Vec<usize>) = (0..queries.len())
        .partition(|&i| queries[i].num_residues >= reference::MIN_RESIDUES);
    let cache = args.work_dir.join("uniprot");
    let DeclaredMatches {
        outcome,
        mut entries,
        mut warnings,
        other_declared,
        rows: blast_rows,
        ..
    } = reference::declared_matches(
        &queries,
        &wanted,
        &args.work_dir.join("no-sifts"),
        &args.work_dir,
        Some(&cache),
    )?;
    let short_accessions: Vec<String> = short
        .iter()
        .flat_map(|&i| reference::declared_accessions(&queries[i]))
        .filter(|acc| !entries.contains_key(acc))
        .collect();
    entries.extend(reference::fetch_entries(
        short_accessions.iter().map(String::as_str),
        Some(&cache),
    ));
    let mut peptides: HashMap<usize, PolymerLookup> = HashMap::new();
    for &i in &short {
        if let Some(lookup) =
            reference::peptide_lookup(&queries[i], &entries, &mut warnings)
        {
            peptides.insert(i, lookup);
        }
    }
    for warning in &warnings {
        warn!("{warning}");
    }

    // Each polymer's accessions that pass, as hits
    let mut passing: HashMap<i64, Vec<PolymerHit>> = HashMap::new();
    for (i, &polymer_id) in ids.iter().enumerate() {
        let mut candidates: Vec<&Candidate> = vec![];
        if let Some(Declared::Match(parts)) = outcome.get(&i) {
            candidates.extend(parts);
        }
        candidates.extend(other_declared.get(&i).into_iter().flatten());
        let mut hits: Vec<PolymerHit> = candidates
            .into_iter()
            .filter_map(|c| match &entries.get(&c.accession) {
                Some(Ok(entry)) => reference::make_hit(c, &queries[i].sequence, entry),
                _ => None,
            })
            .collect();
        if let Some(lookup) = peptides.get(&i) {
            hits.extend(lookup.hits.iter().chain(&lookup.other_declared).cloned());
        }
        passing.insert(polymer_id, hits);
    }

    // The real run writes, a polymer at a time
    let mut written: HashMap<i64, usize> = HashMap::new();
    if let (false, Some(server)) = (args.dry_run, &args.server) {
        let key = dsn_for(server);
        let url = env::var(key).map_err(|e| anyhow!("{key}: {e}"))?;
        let mut conn =
            mdr_db::connect(&url).map_err(|e| anyhow!("Failed to connect: {e}"))?;
        for (&polymer_id, hits) in &passing {
            if hits.is_empty() {
                continue;
            }
            let lookup = PolymerLookup {
                match_method: reference::DECLARED.to_string(),
                hits: vec![],
                other_declared: hits.clone(),
            };
            match conn.transaction(|conn| {
                import::add_declared_references(conn, polymer_id, &lookup)
            }) {
                Ok(n) => {
                    written.insert(polymer_id, n);
                }
                Err(e) => warn!("Polymer {polymer_id}: {e}"),
            }
        }
        info!(
            "Added {} rows to {} polymers",
            written.values().sum::<usize>(),
            written.values().filter(|&&n| n > 0).count()
        );
    }

    // The report, a row per link
    let index: HashMap<i64, usize> =
        ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut links: BTreeMap<(i64, String), Vec<&Polymer>> = BTreeMap::new();
    for row in &rows {
        let link = links
            .entry((row.simulation_id, row.accession.clone()))
            .or_default();
        if let Some(p) = &row.polymer {
            link.push(p);
        }
    }
    let mut out =
        File::create(&args.out).map_err(|e| anyhow!("{}: {e}", args.out.display()))?;
    writeln!(
        out,
        "simulation_id\taccession\tresult\tpolymer_ids\tidentity\tmismatches\t\
         coverage\tdetail"
    )?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for ((sim_id, accession), sim_polymers) in &links {
        let acc = accession.to_uppercase();
        let passed: Vec<i64> = sim_polymers
            .iter()
            .filter(|p| {
                passing[&p.id]
                    .iter()
                    .any(|h| h.uniprot.uniprot_id.to_uppercase() == acc)
            })
            .map(|p| p.id)
            .collect();
        let closest = closest_row(&blast_rows, sim_polymers, &index, &acc);
        let (result, detail) = if sim_polymers.is_empty() {
            (NO_PROTEIN_CHAIN, String::new())
        } else if !passed.is_empty() {
            let detail = if args.dry_run {
                "would be added"
            } else if passed.iter().all(|id| written.contains_key(id)) {
                "added"
            } else {
                "not written: see the log"
            };
            (PASSES, detail.to_string())
        } else {
            match entries.get(&acc) {
                Some(Err(e)) if e.gone => (GONE, e.message.clone()),
                Some(Err(e)) => (UNFINISHED, e.message.clone()),
                _ => (FAILS, String::new()),
            }
        };
        *counts.entry(result).or_default() += 1;
        let polymer_ids = if passed.is_empty() {
            sim_polymers.iter().map(|p| p.id).collect()
        } else {
            passed
        };
        let (identity, mismatches, coverage) = match closest {
            Some(h) => (
                format!("{:.1}", reference::identity(h)),
                h.mismatch.to_string(),
                format!(
                    "{:.1}",
                    100.0 * reference::covered(h) as f64 / h.qlen as f64
                ),
            ),
            None => Default::default(),
        };
        writeln!(
            out,
            "{sim_id}\t{accession}\t{result}\t{}\t{identity}\t{mismatches}\t\
             {coverage}\t{detail}",
            polymer_ids
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )?;
    }
    for (result, count) in &counts {
        info!("{result}: {count}");
    }
    info!("Report: {}", args.out.display());
    Ok(())
}

/// The accession's BLAST row with the top bit score against any of a
/// simulation's polymers: the closest it came, for the report
fn closest_row<'a>(
    rows: &'a [PolymerBlastHit],
    polymers: &[&Polymer],
    index: &HashMap<i64, usize>,
    acc: &str,
) -> Option<&'a PolymerBlastHit> {
    let queries: Vec<String> = polymers
        .iter()
        .filter_map(|p| index.get(&p.id).map(|i| format!("p{i}")))
        .collect();
    rows.iter()
        .filter(|h| {
            queries.contains(&h.qaccver)
                && reference::accession(&h.saccver).to_uppercase() == acc
        })
        .max_by(|a, b| a.bitscore.total_cmp(&b.bitscore))
}

// --------------------------------------------------
fn read_input(path: &Path) -> Result<Vec<Row>> {
    let text =
        fs::read_to_string(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| anyhow!("{}: empty", path.display()))?
        .split('\t')
        .map(str::trim)
        .collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| anyhow!("{}: no column {name}", path.display()))
    };
    let (sim, acc, pid, num, seq) = (
        column("simulation_id")?,
        column("accession")?,
        column("polymer_id")?,
        column("num_residues")?,
        column("sequence")?,
    );
    let mut rows = vec![];
    for (n, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let at = || format!("{} line {}", path.display(), n + 2);
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let field = |i: usize| {
            fields
                .get(i)
                .copied()
                .ok_or_else(|| anyhow!("{}: want {} columns", at(), header.len()))
        };
        let polymer = match field(pid)? {
            "" => None,
            id => Some(Polymer {
                id: id.parse().map_err(|e| anyhow!("{}: {e}", at()))?,
                num_residues: field(num)?
                    .parse()
                    .map_err(|e| anyhow!("{}: {e}", at()))?,
                sequence: field(seq)?.to_string(),
            }),
        };
        rows.push(Row {
            simulation_id: field(sim)?.parse().map_err(|e| anyhow!("{}: {e}", at()))?,
            accession: field(acc)?.to_string(),
            polymer,
        });
    }
    Ok(rows)
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn input_rows_with_and_without_a_polymer() {
        let dir = tempdir().unwrap();
        let tsv = dir.path().join("in.tsv");
        fs::write(
            &tsv,
            "simulation_id\taccession\tpolymer_id\tnum_residues\tsequence\n\
             7\tP0DP23\t11\t40\tMADQ\n\
             8\tQ9XYZ1\t\t\t\n",
        )
        .unwrap();
        let rows = read_input(&tsv).unwrap();
        assert_eq!(
            rows,
            [
                Row {
                    simulation_id: 7,
                    accession: "P0DP23".into(),
                    polymer: Some(Polymer {
                        id: 11,
                        num_residues: 40,
                        sequence: "MADQ".into()
                    }),
                },
                Row {
                    simulation_id: 8,
                    accession: "Q9XYZ1".into(),
                    polymer: None,
                },
            ]
        );
        fs::write(&tsv, "simulation_id\taccession\n7\tP0DP23\n").unwrap();
        assert!(read_input(&tsv).is_err());
    }
}
