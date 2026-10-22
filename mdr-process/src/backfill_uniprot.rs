//! The UniProt backfill (`backfill-polymer-uniprot`, 0.17.0): rewrite the
//! UniProt references of polymers looked up before a fused chain could have
//! one per part (`reference::sifts_parts`). Before 0.17.0 a polymer kept one
//! candidate, so CXCR4 with T4 lysozyme in a loop (3odu) named one of the
//! two, and a fusion no single protein covered half of was `none`.
//!
//! Input is a TSV with a header: `polymer_id`, `pdb_ids` (every PDB ID the
//! polymer's simulations declare, comma-separated, lower case),
//! `match_method`, `num_residues` and `sequence` as the database had them.
//! `mdr-process/sql/backfill-polymer-uniprot-candidates.sql` builds it: the
//! protein polymers of 20 or more residues from a simulation with a declared
//! PDB ID that are `none`, or that leave 20 or more residues outside their
//! one row.
//!
//! Each polymer runs step 2 alone (`reference::declared_matches`), with
//! SIFTS's accessions for all its PDB IDs as candidates and no declared
//! UniProt IDs (the input has none; a polymer they decided is `declared`,
//! and is not written): the parts, else the best single SIFTS candidate as
//! before, else nothing. No search runs: a polymer step 2 does not match is
//! left as it is, so `none` stays `none` and `pdb` is never lost to a
//! search that is not here.
//!
//! Both the dry run and the real run read the database: each polymer's
//! current method and rows, for the report and to check the input is still
//! what the database holds (a polymer that changed since is
//! `stale_input`, never written). Only the real run writes, and only a
//! polymer whose method is `pdb` or `none` and whose proposed rows differ
//! from its rows: in one transaction, the UniProt entries upserted (the
//! rows' foreign key; `new_uniprot` in the report names the ones not in
//! md_uniprot yet), then `ops::replace_polymer_uniprots`, the deliberate
//! override of the import's write-once rule. Others are reported only.
//!
//! UniProt entries and UniRef100 representatives are cached in
//! `<work_dir>/uniprot/`, and BLAST's files go in `<work_dir>/declared/`:
//! give the real run its dry run's work dir and it reads the same entries,
//! so it writes what the dry run reported (BLAST is run again, and is
//! deterministic on the same sequences).
//!
//! Reports: `--out`, one row per polymer, with its flags; `--overlaps-out`,
//! one row per pair of a polymer's SIFTS rows that overlap in our chain,
//! with the share of the shorter it is and what came of it, to check
//! `reference::JUNCTION_SHARE` against.

use crate::{
    import,
    reference::{self, Declared, DeclaredMatches, PolymerQuery},
    ticket::dsn_for,
    types::{BackfillPolymerUniprotArgs, PolymerLookup},
};
use anyhow::{Result, anyhow, bail};
use diesel::{PgConnection, connection::Connection};
use log::{info, warn};
use mdr_db::ops;
use std::{
    collections::{BTreeMap, HashMap},
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

/// The methods the backfill may rewrite: SIFTS's, which it redoes, and
/// `none`, which it may now find. `declared` and `aligned` came from
/// sources it does not run.
const REWRITABLE: [&str; 2] = [reference::PDB, reference::NONE];

/// T4 lysozyme: a receptor fusion that took it as its one reference is the
/// case the backfill is for
const T4L: &str = "P00720";

/// One polymer of the input
#[derive(Debug, Clone, PartialEq)]
struct Row {
    polymer_id: i64,
    pdb_ids: Vec<String>,
    match_method: String,
    num_residues: usize,
    sequence: String,
}

/// A reference as the report shows it
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    accession: String,
    query: (i32, i32),
    reference: (i32, i32),
    identity: f64,
}

impl Shown {
    fn len(&self) -> i32 {
        self.query.1 - self.query.0 + 1
    }

    /// The same part, identity to the report's 0.1
    fn same(&self, other: &Shown) -> bool {
        self.accession == other.accession
            && self.query == other.query
            && self.reference == other.reference
            && (self.identity - other.identity).abs() < 0.05
    }
}

/// What the database holds for a polymer
#[derive(Debug)]
struct Current {
    match_method: Option<String>,
    num_residues: usize,
    sequence: String,
    /// md_polymer_uniprot ids, to see that nothing moved before a write
    row_ids: Vec<i64>,
    rows: Vec<Shown>,
}

// --------------------------------------------------
pub fn backfill_polymer_uniprot(args: &BackfillPolymerUniprotArgs) -> Result<()> {
    let blast_dir = match &args.blast_dir {
        Some(dir) => dir.clone(),
        None => PathBuf::from(
            env::var("MDREPO_WORK_DIR").map_err(|e| anyhow!("MDREPO_WORK_DIR: {e}"))?,
        )
        .join("blast"),
    };
    let sifts = blast_dir.join(reference::PDB_SIFTS);
    if !sifts.is_file() {
        bail!("No PDB index ({})", sifts.display());
    }
    fs::create_dir_all(&args.work_dir)
        .map_err(|e| anyhow!("{}: {e}", args.work_dir.display()))?;

    let rows = read_input(&args.input)?;
    info!("{} polymers", rows.len());

    // Both runs read; only the real one writes
    let key = dsn_for(&args.server);
    let url = env::var(key).map_err(|e| anyhow!("{key}: {e}"))?;
    let mut conn =
        mdr_db::connect(&url).map_err(|e| anyhow!("Failed to connect: {e}"))?;
    let mut current = vec![];
    for row in &rows {
        current.push(read_current(&mut conn, row.polymer_id)?);
    }

    // Step 2 for every polymer the input still describes
    let mut queries = vec![];
    let mut query_of: HashMap<usize, usize> = HashMap::new();
    for (n, (row, cur)) in rows.iter().zip(&current).enumerate() {
        if let Some(cur) = cur
            && is_current(row, cur)
        {
            query_of.insert(n, queries.len());
            queries.push(PolymerQuery {
                polymer_type: "protein".to_string(),
                num_residues: cur.num_residues,
                sequence: cur.sequence.clone(),
                pdb_ids: row.pdb_ids.clone(),
                uniprot_ids: vec![],
            });
        }
    }
    let wanted: Vec<usize> = (0..queries.len())
        .filter(|&i| queries[i].num_residues >= reference::MIN_RESIDUES)
        .collect();
    let cache = args.work_dir.join("uniprot");
    let DeclaredMatches {
        outcome,
        entries,
        mut warnings,
        overlaps,
        ..
    } = reference::declared_matches(
        &queries,
        &wanted,
        &sifts,
        &args.work_dir,
        Some(&cache),
    )?;
    let lookups: Vec<Option<PolymerLookup>> = queries
        .iter()
        .enumerate()
        .map(|(i, q)| match outcome.get(&i) {
            Some(Declared::Match(parts)) => {
                reference::finish(parts, &q.sequence, &entries, &mut warnings)
            }
            Some(Declared::Unfinished) => None,
            Some(Declared::NoMatch) | None => Some(reference::none()),
        })
        .collect();
    for warning in &warnings {
        warn!("{warning}");
    }

    let mut out =
        File::create(&args.out).map_err(|e| anyhow!("{}: {e}", args.out.display()))?;
    writeln!(
        out,
        "polymer_id\tpdb_ids\tnum_residues\tcurrent_method\tcurrent_rows\t\
         proposed_method\tproposed_rows\tcoverage_before\tcoverage_after\t\
         new_uniprot\tflags\tstatus"
    )?;
    let mut flag_counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (n, row) in rows.iter().enumerate() {
        let cur = current[n].as_ref();
        let lookup = query_of.get(&n).and_then(|&i| lookups[i].as_ref());
        let current_rows = cur.map(|c| c.rows.clone()).unwrap_or_default();
        let proposed: Vec<Shown> = lookup.map_or(vec![], shown);
        let num_residues = cur.map_or(row.num_residues, |c| c.num_residues);
        let flags = flags(row, cur, lookup, &proposed);
        for flag in &flags {
            *flag_counts.entry(flag).or_default() += 1;
        }
        let new_uniprot: Vec<&str> = {
            let mut accs: Vec<&str> =
                proposed.iter().map(|p| p.accession.as_str()).collect();
            accs.sort_unstable();
            accs.dedup();
            let mut new = vec![];
            for acc in accs {
                if ops::find_uniprot_id_by_accession(&mut conn, acc)?.is_none() {
                    new.push(acc);
                }
            }
            new
        };

        let writable = !flags.iter().any(|f| {
            [
                STALE_INPUT,
                SKIPPED_NOT_PDB_OR_NONE,
                UNFINISHED,
                UNCHANGED,
                NOT_FOUND_NOW,
            ]
            .contains(f)
        }) && !proposed.is_empty();
        let status = match (writable, args.dry_run, lookup, cur) {
            (false, ..) => "not written".to_string(),
            (true, true, ..) => "would write".to_string(),
            (true, false, Some(lookup), Some(cur)) => {
                match write(&mut conn, row.polymer_id, cur, lookup) {
                    Ok(()) => "written".to_string(),
                    Err(e) => {
                        warn!("Polymer {}: {e}", row.polymer_id);
                        format!("failed: {e}")
                    }
                }
            }
            (true, false, ..) => "not written".to_string(),
        };
        *status_counts
            .entry(status.split(':').next().unwrap_or("").to_string())
            .or_default() += 1;

        writeln!(
            out,
            "{}\t{}\t{num_residues}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{status}",
            row.polymer_id,
            row.pdb_ids.join(","),
            cur.and_then(|c| c.match_method.as_deref()).unwrap_or(""),
            show_rows(&current_rows),
            lookup.map_or("untried", |l| l.match_method.as_str()),
            show_rows(&proposed),
            coverage(&current_rows, num_residues),
            if lookup.is_some() {
                coverage(&proposed, num_residues)
            } else {
                String::new()
            },
            new_uniprot.join(","),
            flags.join(","),
        )?;
    }

    write_overlaps(&args.overlaps_out, &rows, &query_of, &overlaps)?;
    for (flag, count) in &flag_counts {
        info!("{flag}: {count}");
    }
    for (status, count) in &status_counts {
        info!("{status}: {count}");
    }
    info!(
        "Reports: {} and {}",
        args.out.display(),
        args.overlaps_out.display()
    );
    Ok(())
}

/// Flags of a polymer, for the report
const NONE_TO_FOUND: &str = "none_to_found";
const T4L_ONLY_GAINS_PARTS: &str = "t4l_only_gains_parts";
const GAINS_PARTS: &str = "gains_parts";
const FIRST_ACCESSION_CHANGES: &str = "first_accession_changes";
const COVERAGE_SHRINKS: &str = "coverage_shrinks";
const UNCHANGED: &str = "unchanged";
const NOT_FOUND_NOW: &str = "not_found_now";
const SKIPPED_NOT_PDB_OR_NONE: &str = "skipped_not_pdb_or_none";
const SEVERAL_PDB_IDS: &str = "several_pdb_ids";
const STALE_INPUT: &str = "stale_input";
const UNFINISHED: &str = "unfinished";

/// What a polymer's proposed rows do to its current ones.
/// `not_found_now`: it has rows, and step 2 now matches nothing (not
/// written: the search that might have found them does not run here).
/// `unfinished`: a UniProt entry could not be fetched.
fn flags(
    row: &Row,
    cur: Option<&Current>,
    lookup: Option<&PolymerLookup>,
    proposed: &[Shown],
) -> Vec<&'static str> {
    let mut flags = vec![];
    if row.pdb_ids.len() > 1 {
        flags.push(SEVERAL_PDB_IDS);
    }
    let Some(cur) = cur.filter(|c| is_current(row, c)) else {
        flags.push(STALE_INPUT);
        return flags;
    };
    let method = cur.match_method.as_deref().unwrap_or("");
    if !REWRITABLE.contains(&method) {
        flags.push(SKIPPED_NOT_PDB_OR_NONE);
    }
    let Some(_) = lookup else {
        flags.push(UNFINISHED);
        return flags;
    };
    let before = &cur.rows;
    if before.len() == proposed.len()
        && before.iter().zip(proposed).all(|(a, b)| a.same(b))
    {
        flags.push(UNCHANGED);
        return flags;
    }
    if proposed.is_empty() {
        flags.push(NOT_FOUND_NOW);
        return flags;
    }
    if before.is_empty() {
        flags.push(NONE_TO_FOUND);
        return flags;
    }
    if proposed.len() > before.len() {
        flags.push(GAINS_PARTS);
        if before.len() == 1 && before[0].accession == T4L {
            flags.push(T4L_ONLY_GAINS_PARTS);
        }
    }
    if before[0].accession != proposed[0].accession {
        flags.push(FIRST_ACCESSION_CHANGES);
    }
    let total = |rows: &[Shown]| rows.iter().map(Shown::len).sum::<i32>();
    if total(proposed) < total(before) {
        flags.push(COVERAGE_SHRINKS);
    }
    flags
}

/// The input still describes the polymer as the database holds it
fn is_current(row: &Row, cur: &Current) -> bool {
    cur.match_method.as_deref() == Some(row.match_method.as_str())
        && cur.num_residues == row.num_residues
        && cur.sequence == row.sequence
}

/// A polymer's method and rows, None when there is no such polymer
fn read_current(conn: &mut PgConnection, polymer_id: i64) -> Result<Option<Current>> {
    let polymer = match ops::get_polymer(conn, polymer_id) {
        Ok(p) => p,
        Err(diesel::result::Error::NotFound) => return Ok(None),
        Err(e) => bail!("Polymer {polymer_id}: {e}"),
    };
    let parts = ops::list_polymer_uniprots(conn, polymer_id)?;
    let mut rows = vec![];
    for part in &parts {
        rows.push(Shown {
            accession: ops::get_uniprot(conn, part.uniprot_id)?.uniprot_id,
            query: (part.query_start, part.query_end),
            reference: (part.reference_start, part.reference_end),
            identity: part.identity,
        });
    }
    Ok(Some(Current {
        match_method: polymer.match_method,
        num_residues: usize::try_from(polymer.num_residues)?,
        sequence: polymer.sequence,
        row_ids: parts.iter().map(|p| p.id).collect(),
        rows,
    }))
}

/// The real run's write for one polymer, in one transaction: refused if
/// the polymer's method or rows changed since they were read; the UniProt
/// entries upserted (`import::polymer_references`), then the rows replaced
fn write(
    conn: &mut PgConnection,
    polymer_id: i64,
    read: &Current,
    lookup: &PolymerLookup,
) -> Result<()> {
    conn.transaction(|conn| {
        let now = read_current(conn, polymer_id)?
            .ok_or_else(|| anyhow!("no longer in the database"))?;
        if now.match_method != read.match_method || now.row_ids != read.row_ids {
            bail!("changed since it was read; not written");
        }
        let parts = import::polymer_references(conn, lookup)?;
        ops::replace_polymer_uniprots(conn, polymer_id, &lookup.match_method, &parts)?;
        Ok(())
    })
}

fn shown(lookup: &PolymerLookup) -> Vec<Shown> {
    lookup
        .hits
        .iter()
        .map(|h| Shown {
            accession: h.uniprot.uniprot_id.clone(),
            query: (h.query_start, h.query_end),
            reference: (h.reference_start, h.reference_end),
            identity: h.identity,
        })
        .collect()
}

/// `P61073:1-228[12-239,98.2];P00720:231-390[2-161,100.0]`: accession, our
/// range, the entry's range and the identity of each part
fn show_rows(rows: &[Shown]) -> String {
    rows.iter()
        .map(|r| {
            format!(
                "{}:{}-{}[{}-{},{:.1}]",
                r.accession,
                r.query.0,
                r.query.1,
                r.reference.0,
                r.reference.1,
                r.identity
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The percent of the polymer the rows' query ranges cover together
fn coverage(rows: &[Shown], num_residues: usize) -> String {
    if num_residues == 0 {
        return String::new();
    }
    let total: i32 = rows.iter().map(Shown::len).sum();
    format!("{:.1}", 100.0 * total as f64 / num_residues as f64)
}

/// One row per overlapping pair of a polymer's SIFTS rows
fn write_overlaps(
    path: &Path,
    rows: &[Row],
    query_of: &HashMap<usize, usize>,
    overlaps: &HashMap<usize, Vec<reference::Overlap>>,
) -> Result<()> {
    let mut out = File::create(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    writeln!(
        out,
        "polymer_id\tfirst\tsecond\tsame_accession\toverlap\tshare\toutcome\t\
         junction_overlap\tjunction_share"
    )?;
    for (n, row) in rows.iter().enumerate() {
        let Some(pairs) = query_of.get(&n).and_then(|i| overlaps.get(i)) else {
            continue;
        };
        for pair in pairs {
            writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{}\t{}",
                row.polymer_id,
                pair.first,
                pair.second,
                pair.same_accession,
                pair.length,
                pair.share,
                pair.outcome,
                reference::JUNCTION_OVERLAP,
                reference::JUNCTION_SHARE
            )?;
        }
    }
    Ok(())
}

/// The input TSV, its columns found by the header's names
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
    let (id, pdb, method, num, seq) = (
        column("polymer_id")?,
        column("pdb_ids")?,
        column("match_method")?,
        column("num_residues")?,
        column("sequence")?,
    );
    let mut rows = vec![];
    for (n, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let field = |i: usize| {
            fields.get(i).map(|f| f.trim()).ok_or_else(|| {
                anyhow!(
                    "{} line {}: want {} columns",
                    path.display(),
                    n + 2,
                    header.len()
                )
            })
        };
        let parse_err = |e: std::num::ParseIntError| {
            anyhow!("{} line {}: {e}", path.display(), n + 2)
        };
        rows.push(Row {
            polymer_id: field(id)?.parse().map_err(parse_err)?,
            pdb_ids: field(pdb)?
                .split(',')
                .map(|p| p.trim().to_lowercase())
                .filter(|p| !p.is_empty())
                .collect(),
            match_method: field(method)?.to_string(),
            num_residues: field(num)?.parse().map_err(parse_err)?,
            sequence: field(seq)?.to_string(),
        });
    }
    Ok(rows)
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn shown(acc: &str, q: (i32, i32)) -> Shown {
        Shown {
            accession: acc.into(),
            query: q,
            reference: (q.0 + 10, q.1 + 10),
            identity: 99.0,
        }
    }

    fn row(pdb_ids: &[&str], method: &str) -> Row {
        Row {
            polymer_id: 1,
            pdb_ids: pdb_ids.iter().map(|p| p.to_string()).collect(),
            match_method: method.into(),
            num_residues: 300,
            sequence: "A".repeat(300),
        }
    }

    fn current(method: &str, rows: Vec<Shown>) -> Current {
        Current {
            match_method: Some(method.into()),
            num_residues: 300,
            sequence: "A".repeat(300),
            row_ids: vec![],
            rows,
        }
    }

    fn lookup(method: &str) -> PolymerLookup {
        PolymerLookup {
            match_method: method.into(),
            hits: vec![],
        }
    }

    #[test]
    fn flags_of_a_polymer() {
        let pdb = lookup("pdb");
        let flag = |r: &Row, c: &Current, proposed: &[Shown]| {
            flags(r, Some(c), Some(&pdb), proposed)
        };
        let t4l = vec![shown(T4L, (100, 260))];
        let fused = vec![
            shown("P61073", (1, 98)),
            shown(T4L, (100, 260)),
            shown("P61073", (263, 300)),
        ];

        // A receptor fusion that had T4L alone
        assert_eq!(
            flag(&row(&["3odu"], "pdb"), &current("pdb", t4l.clone()), &fused),
            [GAINS_PARTS, T4L_ONLY_GAINS_PARTS, FIRST_ACCESSION_CHANGES]
        );
        // Found where nothing was, by two PDB IDs
        assert_eq!(
            flag(
                &row(&["3odu", "4rws"], "none"),
                &current("none", vec![]),
                &fused
            ),
            [SEVERAL_PDB_IDS, NONE_TO_FOUND]
        );
        // The same rows
        assert_eq!(
            flag(
                &row(&["3odu"], "pdb"),
                &current("pdb", fused.clone()),
                &fused
            ),
            [UNCHANGED]
        );
        // Shorter, and nothing at all
        let short = vec![shown(T4L, (110, 250))];
        assert_eq!(
            flag(&row(&["3odu"], "pdb"), &current("pdb", t4l.clone()), &short),
            [COVERAGE_SHRINKS]
        );
        assert_eq!(
            flag(&row(&["3odu"], "pdb"), &current("pdb", t4l.clone()), &[]),
            [NOT_FOUND_NOW]
        );
        // Declared: reported, flagged so it is not written
        assert_eq!(
            flag(
                &row(&["3odu"], "declared"),
                &current("declared", t4l.clone()),
                &fused
            )[0],
            SKIPPED_NOT_PDB_OR_NONE
        );
        // The database moved on since the input was made
        assert_eq!(
            flag(
                &row(&["3odu"], "none"),
                &current("pdb", t4l.clone()),
                &fused
            ),
            [STALE_INPUT]
        );
        assert_eq!(
            flags(&row(&["3odu"], "pdb"), None, None, &[]),
            [STALE_INPUT]
        );
        // A UniProt fetch failed
        assert_eq!(
            flags(
                &row(&["3odu"], "pdb"),
                Some(&current("pdb", t4l)),
                None,
                &[]
            ),
            [UNFINISHED]
        );
    }

    #[test]
    fn report_columns() {
        let rows = vec![
            Shown {
                identity: 98.24,
                ..shown("P61073", (1, 228))
            },
            shown(T4L, (231, 390)),
        ];
        assert_eq!(
            show_rows(&rows),
            "P61073:1-228[11-238,98.2];P00720:231-390[241-400,99.0]"
        );
        assert_eq!(coverage(&rows, 482), "80.5");
        assert_eq!(coverage(&[], 482), "0.0");
    }

    #[test]
    fn reads_the_candidates_tsv() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("candidates.tsv");
        fs::write(
            &file,
            "polymer_id\tpdb_ids\tmatch_method\tnum_residues\tsequence\n\
             11684\t3odu,3OE0\tpdb\t5\tACDEF\n\n",
        )
        .unwrap();
        let rows = read_input(&file).unwrap();
        assert_eq!(
            rows,
            [Row {
                polymer_id: 11684,
                pdb_ids: vec!["3odu".into(), "3oe0".into()],
                match_method: "pdb".into(),
                num_residues: 5,
                sequence: "ACDEF".into(),
            }]
        );

        fs::write(&file, "polymer_id\tpdb_ids\n1\t3odu\n").unwrap();
        let err = read_input(&file).unwrap_err().to_string();
        assert!(err.contains("no column match_method"), "{err}");
    }
}
