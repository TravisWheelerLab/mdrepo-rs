//! A polymer's UniProt reference. Per distinct polymer in a simulation:
//!
//! 1. Not protein, or under `MIN_RESIDUES` residues: `none`. BLAST cannot
//!    reach its e-value with a short query, and a short one often matches
//!    many entries identically.
//! 2. **The declared candidates** (Ken, 2026-10-13): the UniProt entries
//!    SIFTS (`pdb_chain_uniprot.tsv`) lists for the simulation's declared PDB
//!    entry (`pdb`), and the simulation's declared UniProt IDs (`declared`;
//!    `pdb` when SIFTS lists the same entry). Curated sources say *which*
//!    entries; BLAST says where they match: each candidate is compared with
//!    the chain by `blastp -subject`, with no database. A SIFTS accession is
//!    PDBe's assignment and is taken whenever BLAST aligns it over at least
//!    `MIN_SIFTS_COVERAGE` of the chain (a guard against a wrong declared
//!    PDB ID) at `MIN_SIFTS_IDENTITY` or more, in one piece or in several
//!    (`sifts_coverage`: a protein with an insert), with no mismatch limit, so a
//!    mutant or engineered construct keeps its entry, as in 0.6.0 (Ken,
//!    2026-10-13, after the DDD dry run); a declared UniProt ID must pass the
//!    pass rule below. The identity floor is there because SIFTS lists
//!    entries per PDB entry, not per chain: without it an antibody's heavy
//!    chain took its light chain's entry at 26.7% (1etz, 2026-10-14).
//! 3. **The search** (`aligned`): Swiss-Prot, then TrEMBL for what Swiss-Prot
//!    did not pass.
//! 4. Otherwise `none`.
//!
//! A hit passes with identity >= 95%, at most 3 mismatches, and the
//! alignment covering >= 90% of the chain, for declared UniProt IDs in step
//! 2 and in step 3 (Ken, 2026-10-13). Identity is the share of our aligned residues that are
//! identical (`identity`), not BLAST's `pident`: residues our chain lacks, an
//! unresolved loop, are gaps in BLAST's alignment, and must not count
//! against it (Ken, 2026-10-13). The best passing hit has the top bit score. Ties go to the
//! UniRef100 representative when it is one of the tied entries, so a sequence
//! that several strains' entries share gets the cluster's (SOL3's S288c
//! P38858, not YJM789's A6ZT71); then to a canonical entry over an isoform;
//! then to the lowest accession. The query and reference ranges and the
//! identity are BLAST's for every method.
//!
//! The reference is the polymer's (md_polymer_uniprot), shared by every
//! simulation with the same residues, and set once: the import writes it only to a polymer never
//! looked up (`ops::set_polymer_reference`). Step 2 reads the simulation's
//! own PDB and UniProt IDs, so simulations sharing a polymer can disagree;
//! the first to import decides, and a later one that disagrees gets a
//! warning (`import::reference_disagreements`). Ken relaxed "no simulation
//! may point to the wrong UniProt entry" to allow this (2026-10-09). Every
//! declared UniProt ID is linked to the simulation regardless.
//!
//! Nothing here writes to the database. A polymer whose lookup could not be
//! finished (no PDB index on the host, a UniProt fetch that failed) gets no
//! result at all and is left for a later run, rather than a `none` that
//! would stick.

use crate::{
    process::{
        blast_db, get_uniprot_entry, run_blastp, run_blastp_subject,
        uniprot_entry_from_json,
    },
    types::{ImportChain, PolymerHit, PolymerLookup, UniprotDb, UniprotEntry},
};
use anyhow::{Result, anyhow, bail};
use libmdrepo::common::file_exists;
use log::{debug, info, warn};
use rayon::prelude::*;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path,
};

/// UniProt entries fetched at once by a batch
const UNIPROT_FETCHES: usize = 8;

/// Shorter chains get `none` (question a, Ken, 2026-10-09)
pub const MIN_RESIDUES: usize = 20;

const MIN_IDENTITY: f64 = 95.0;
const MAX_MISMATCHES: u32 = 3;
const MIN_COVERAGE: f64 = 0.9;

/// A SIFTS accession skips the pass rule but must cover this much of the
/// chain, against a simulation that declares the wrong PDB ID (Ken,
/// 2026-10-13: "50% until things go sideways")
const MIN_SIFTS_COVERAGE: f64 = 0.5;

/// A SIFTS accession must also be this identical, so a chain does not take
/// another chain's entry in the same PDB entry (the report's question 2,
/// taken as suggested by Ken, 2026-10-14: it removed exactly the 25
/// wrong-chain matches in the DDD dry run)
const MIN_SIFTS_IDENTITY: f64 = 70.0;

/// Enough targets that every hit tied at the top bit score is listed, so the
/// tie-break sees them all: calmodulin's sequence ties more than 20 Swiss-Prot
/// entries, and at the old 20 CALM1 (P0DP23) was cut from the list
const MAX_TARGET_SEQS: &str = "500";

/// Plain `-outfmt 6` plus the lengths the coverage floor needs and the
/// count of identical residues `identity` needs
const BLAST_OUTFMT: &str = "6 qaccver saccver pident length mismatch gapopen \
                            qstart qend sstart send evalue bitscore qlen slen \
                            nident";

/// A tie's entries asked for their UniRef100 representative, at most
const UNIREF_TRIES: usize = 5;

/// SIFTS's chain-to-UniProt segments (`pdb_chain_uniprot.tsv`),
/// uncompressed, under the BLAST dir and refreshed with it (`just pdb-index`)
pub const PDB_SIFTS: &str = "pdb/pdb_chain_uniprot.tsv";

pub const PDB: &str = "pdb";
pub const DECLARED: &str = "declared";
pub const ALIGNED: &str = "aligned";
pub const NONE: &str = "none";

// --------------------------------------------------
/// A reference before its UniProt entry is fetched: BLAST's row for it,
/// with `identity` as `identity()` measures it
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub match_method: &'static str,
    pub accession: String,
    pub query_start: i32,
    pub query_end: i32,
    pub reference_start: i32,
    pub reference_end: i32,
    pub identity: f64,
}

// --------------------------------------------------
/// One row of `BLAST_OUTFMT`
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PolymerBlastHit {
    pub qaccver: String,
    pub saccver: String,
    pub pident: f64,
    pub length: u32,
    pub mismatch: u32,
    pub gapopen: u32,
    pub qstart: u32,
    pub qend: u32,
    pub sstart: u32,
    pub send: u32,
    pub evalue: f64,
    pub bitscore: f64,
    pub qlen: u32,
    pub slen: u32,
    pub nident: u32,
}

// --------------------------------------------------
/// One polymer to look up: what it is, and the declared PDB ID and UniProt
/// IDs of the simulation that decides it
#[derive(Debug, Clone, PartialEq)]
pub struct PolymerQuery {
    pub polymer_type: String,
    pub num_residues: usize,
    pub sequence: String,
    pub pdb_id: Option<String>,
    pub uniprot_ids: Vec<String>,
}

/// `lookup_polymers`'s result: a lookup per query, warnings, and the
/// queries related to their declared PDB entry
pub type Lookups = (Vec<Option<PolymerLookup>>, Vec<String>, HashSet<usize>);

/// `declared_matches`'s result: step 2 per polymer, the entries fetched,
/// warnings, and the polymers related to their declared PDB entry
pub type DeclaredMatches = (
    HashMap<usize, Declared>,
    Entries,
    Vec<String>,
    HashSet<usize>,
);

// --------------------------------------------------
/// Step 2's outcome for one polymer that has candidates
#[derive(Debug, Clone, PartialEq)]
pub enum Declared {
    /// The best candidate that passed
    Match(Candidate),
    /// None passed: on to the search
    NoMatch,
    /// A candidate's UniProt entry could not be fetched, so this is not
    /// known
    Unfinished,
}

/// UniProt entries by accession, or why one could not be had
pub type Entries = HashMap<String, std::result::Result<UniprotEntry, String>>;

// --------------------------------------------------
/// Look up every distinct polymer among one simulation's `chains` and set
/// each chain's `reference`. `pdb_id` and `uniprot_ids` are the
/// simulation's declared IDs. Returns warnings for the submitter, among them
/// the PDB ID check (`declared_pdb_not_matched`).
pub fn lookup_chains(
    chains: &mut [ImportChain],
    pdb_id: Option<&str>,
    uniprot_ids: &[String],
    blast_dir: &Path,
    processed_dir: &Path,
    num_threads: usize,
) -> Result<Vec<String>> {
    // Distinct polymers, in chain order: (type, residues) is md_polymer's
    // key. The first chain of each gives the sequence.
    let mut keys: Vec<(String, Vec<String>)> = vec![];
    let mut queries: Vec<PolymerQuery> = vec![];
    for chain in chains.iter() {
        let key = (chain.polymer_type.clone(), chain.residues.clone());
        if !keys.contains(&key) {
            keys.push(key);
            queries.push(PolymerQuery {
                polymer_type: chain.polymer_type.clone(),
                num_residues: chain.residues.len(),
                sequence: chain.sequence.clone(),
                pdb_id: pdb_id.map(str::to_string),
                uniprot_ids: uniprot_ids.to_vec(),
            });
        }
    }

    let (lookups, mut warnings, related) =
        lookup_polymers(&queries, blast_dir, processed_dir, num_threads, None)?;

    for chain in chains.iter_mut() {
        let i = keys
            .iter()
            .position(|k| (&k.0, &k.1) == (&chain.polymer_type, &chain.residues))
            .ok_or_else(|| anyhow!("chain {} has no polymer", chain.chain_order))?;
        chain.reference = lookups[i].clone();
    }

    // The PDB ID check; SIFTS is read again only when it would warn
    if let Some(pdb_id) = pdb_id {
        let flags = related_flags(&queries, &lookups, &related);
        let sifts = blast_dir.join(PDB_SIFTS);
        if none_related(&flags) && sifts.is_file() {
            let code = pdb_id.to_lowercase();
            let listed = sifts_accessions(&sifts, &[code.clone()].into())?
                .remove(&code)
                .unwrap_or_default();
            warnings.extend(declared_pdb_not_matched(pdb_id, &listed, &flags));
        }
    }

    Ok(warnings)
}

// --------------------------------------------------
/// Steps 1-4 for many polymers at once, each against its own declared IDs:
/// SIFTS is read once, the search runs once over every polymer that needs
/// it (its files go in `work_dir`), and each UniProt entry is fetched once
/// (or read from `uniprot_cache`, see `fetch_entries`). Returns a lookup per
/// query, None where it could not be finished (left untried), warnings, and
/// the queries related to their declared PDB entry (`related_to_pdb`).
pub fn lookup_polymers(
    queries: &[PolymerQuery],
    blast_dir: &Path,
    work_dir: &Path,
    num_threads: usize,
    uniprot_cache: Option<&Path>,
) -> Result<Lookups> {
    let mut warnings = vec![];
    let mut lookups: Vec<Option<PolymerLookup>> = vec![None; queries.len()];

    // 1. Not protein, or short
    let mut wanted = vec![];
    for (i, q) in queries.iter().enumerate() {
        if q.polymer_type == "protein" && q.num_residues >= MIN_RESIDUES {
            wanted.push(i);
        } else {
            lookups[i] = Some(none());
        }
    }
    if wanted.is_empty() {
        return Ok((lookups, warnings, HashSet::new()));
    }

    let sifts = blast_dir.join(PDB_SIFTS);
    if !sifts.is_file() {
        warnings.push(format!(
            "The UniProt references of the chains were not looked up: no PDB \
             index ({})",
            sifts.display()
        ));
        return Ok((lookups, warnings, HashSet::new()));
    }

    // 2. The declared candidates
    let (declared, mut entries, declared_warnings, related) =
        declared_matches(queries, &wanted, &sifts, work_dir, uniprot_cache)?;
    warnings.extend(declared_warnings);
    let mut to_search = vec![];
    for &i in &wanted {
        match declared.get(&i) {
            Some(Declared::Match(candidate)) => {
                lookups[i] =
                    finish(candidate, &queries[i].sequence, &entries, &mut warnings);
            }
            Some(Declared::Unfinished) => {}
            Some(Declared::NoMatch) | None => to_search.push(i),
        }
    }

    // 3. The search, once for all
    if !to_search.is_empty() {
        let blast_queries: Vec<(String, &str)> = to_search
            .iter()
            .map(|&i| (format!("p{i}"), queries[i].sequence.as_str()))
            .collect();
        let found = blast_candidates(
            &blast_queries,
            blast_dir,
            work_dir,
            num_threads,
            uniprot_cache,
        )?;
        let more = fetch_entries(
            found
                .values()
                .map(|c| c.accession.as_str())
                .filter(|acc| !entries.contains_key(*acc)),
            uniprot_cache,
        );
        entries.extend(more);
        for &i in &to_search {
            lookups[i] = match found.get(&format!("p{i}")) {
                Some(candidate) => {
                    finish(candidate, &queries[i].sequence, &entries, &mut warnings)
                }
                // 4. Nothing passed
                None => Some(none()),
            };
        }
    }

    Ok((lookups, warnings, related))
}

/// A candidate's lookup once its UniProt entry is in hand; `none` when
/// BLAST's range is past the entry's sequence (a search database older than
/// the release UniProt serves now); None, with a warning, when the entry
/// could not be fetched
fn finish(
    candidate: &Candidate,
    sequence: &str,
    entries: &Entries,
    warnings: &mut Vec<String>,
) -> Option<PolymerLookup> {
    match &entries[&candidate.accession] {
        Ok(entry) => Some(match make_hit(candidate, sequence, entry) {
            Some(hit) => lookup(candidate, hit),
            None => {
                debug!(
                    "{}'s range {}-{} is past its sequence; no reference",
                    candidate.accession,
                    candidate.reference_start,
                    candidate.reference_end
                );
                none()
            }
        }),
        Err(e) => {
            warnings.push(not_recorded(candidate, e));
            None
        }
    }
}

// --------------------------------------------------
/// Step 2 for each of `wanted`: its candidates (the entries SIFTS lists for
/// its declared PDB ID, and its declared UniProt IDs), fetched and compared
/// with its sequence by BLAST. Polymers with the same candidates share one
/// `blastp`, whose files go in `<work_dir>/declared/`. A polymer with no
/// candidates is absent from the result. Also returns the entries fetched,
/// warnings, and the polymers related to their declared PDB entry
/// (`related_to_pdb`).
pub fn declared_matches(
    queries: &[PolymerQuery],
    wanted: &[usize],
    sifts: &Path,
    work_dir: &Path,
    uniprot_cache: Option<&Path>,
) -> Result<DeclaredMatches> {
    let pdb_ids: HashSet<String> = wanted
        .iter()
        .filter_map(|&i| queries[i].pdb_id.as_ref().map(|p| p.to_lowercase()))
        .collect();
    let listed = sifts_accessions(sifts, &pdb_ids)?;

    // Each polymer's candidates: accession -> match_method
    let mut candidates: BTreeMap<usize, BTreeMap<String, &'static str>> =
        BTreeMap::new();
    for &i in wanted {
        let q = &queries[i];
        let mut found = BTreeMap::new();
        for acc in &q.uniprot_ids {
            let acc = acc.trim().to_uppercase();
            if !acc.is_empty() {
                found.insert(acc, DECLARED);
            }
        }
        if let Some(accs) = q
            .pdb_id
            .as_ref()
            .and_then(|p| listed.get(&p.to_lowercase()))
        {
            for acc in accs {
                found.insert(acc.clone(), PDB);
            }
        }
        if !found.is_empty() {
            candidates.insert(i, found);
        }
    }
    let entries = fetch_entries(
        candidates
            .values()
            .flat_map(|found| found.keys().map(String::as_str)),
        uniprot_cache,
    );

    // By candidates, the polymers to compare with them
    let mut warnings = vec![];
    let mut outcome = HashMap::new();
    let mut groups: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
    for (&i, found) in &candidates {
        let failed: Vec<String> = found
            .keys()
            .filter_map(|acc| {
                entries[acc].as_ref().err().map(|e| format!("{acc} ({e})"))
            })
            .collect();
        if failed.is_empty() {
            groups
                .entry(found.keys().cloned().collect())
                .or_default()
                .push(i);
        } else {
            warnings.push(format!(
                "A chain's UniProt reference was not looked up: a declared \
                 candidate could not be fetched: {}",
                failed.join(", ")
            ));
            outcome.insert(i, Declared::Unfinished);
        }
    }

    let dir = work_dir.join("declared");
    if !groups.is_empty() {
        fs::create_dir_all(&dir).map_err(|e| anyhow!("{}: {e}", dir.display()))?;
    }
    let groups: Vec<(Vec<String>, Vec<usize>)> = groups.into_iter().collect();
    let compared: Vec<Result<Vec<PolymerBlastHit>>> = groups
        .par_iter()
        .enumerate()
        .map(|(n, (accessions, members))| {
            compare(&dir, n, queries, members, accessions, &entries)
        })
        .collect();
    let mut rows = vec![];
    for hits in compared {
        rows.extend(hits?);
    }
    // A SIFTS accession passes when BLAST aligns it over half the chain at
    // 70% identity or more, in one piece or several (`sifts_coverage`)
    let sifts_listed: HashSet<(String, String)> = candidates
        .iter()
        .flat_map(|(&i, found)| {
            found
                .iter()
                .filter(|(_, method)| **method == PDB)
                .map(move |(acc, _)| (format!("p{i}"), acc.clone()))
        })
        .collect();
    let coverage = sifts_coverage(&rows, &sifts_listed);
    let related = related_to_pdb(&rows, &sifts_listed);
    let pass = |hit: &PolymerBlastHit| passes_sifts(hit, &coverage) || passes(hit);
    let representatives = representatives(&top_ties_by(&rows, &pass), uniprot_cache);
    let best = best_hits_by(&rows, &representatives, &pass);

    for (&i, found) in &candidates {
        if outcome.contains_key(&i) {
            continue;
        }
        let result = match best.get(&format!("p{i}")) {
            Some(c) => Declared::Match(Candidate {
                match_method: found[&c.accession],
                ..c.clone()
            }),
            None => Declared::NoMatch,
        };
        outcome.insert(i, result);
    }
    Ok((outcome, entries, warnings, related))
}

/// The polymers (`p<i>` -> i) that some SIFTS accession of their declared
/// PDB entry aligns to at `MIN_SIFTS_IDENTITY` or more over `MIN_RESIDUES`
/// or more of our residues: the entry is about this protein, whether or not
/// it becomes the reference. Looser than the SIFTS rule on purpose, for the
/// PDB ID check (`declared_pdb_not_matched`): an antibody whose entry SIFTS
/// maps only over its constant half, or a chain whose declared UniProt ID
/// outscores the SIFTS accession (1hxw), still has the right PDB ID.
fn related_to_pdb(
    rows: &[PolymerBlastHit],
    listed: &HashSet<(String, String)>,
) -> HashSet<usize> {
    rows.iter()
        .filter(|h| {
            listed.contains(&(h.qaccver.clone(), accession(&h.saccver)))
                && identity(h) >= MIN_SIFTS_IDENTITY
                && covered(h) as usize >= MIN_RESIDUES
        })
        .filter_map(|h| h.qaccver.strip_prefix('p')?.parse().ok())
        .collect()
}

/// For each (FASTA ID, accession) in `listed`, our residues that its BLAST
/// rows at `MIN_SIFTS_IDENTITY` or more cover together. A protein with
/// another inserted in a loop (CXCR4 with T4 lysozyme, 3odu) comes back
/// from BLAST as two rows, one each side of the insert, each under half the
/// chain; together they are 63% of it (sim 10488, 2026-10-15). The rows are
/// taken by bit score; a row counts only if it runs in the same order as
/// those counted before, in our chain and in the entry, and only for what
/// it adds: residues already counted, in our chain or in the entry, are
/// taken off (BLAST's rows can share a few residues at the insert). So a
/// repeat cannot count twice: a second row on the same residues of the
/// entry adds nothing. The total is never less than the longest row.
fn sifts_coverage(
    rows: &[PolymerBlastHit],
    listed: &HashSet<(String, String)>,
) -> HashMap<(String, String), u32> {
    let span = |a: u32, b: u32| (a.min(b), a.max(b));
    let overlap =
        |x: (u32, u32), y: (u32, u32)| (x.1.min(y.1) + 1).saturating_sub(x.0.max(y.0));
    let mut by_pair: HashMap<(String, String), Vec<&PolymerBlastHit>> = HashMap::new();
    for hit in rows {
        let pair = (hit.qaccver.clone(), accession(&hit.saccver));
        if listed.contains(&pair) && identity(hit) >= MIN_SIFTS_IDENTITY {
            by_pair.entry(pair).or_default().push(hit);
        }
    }
    by_pair
        .into_iter()
        .map(|(pair, mut hits)| {
            hits.sort_by(|a, b| {
                b.bitscore
                    .total_cmp(&a.bitscore)
                    .then_with(|| a.qstart.cmp(&b.qstart))
            });
            // Never less than the longest row alone, which counted before
            // 0.12.0: a tethered dimer (HIV protease, 3nxn) aligns both
            // copies to the same residues of the entry, and the row taken
            // first can be the shorter one, the other then out of order
            let longest = hits
                .iter()
                .map(|h| h.qend.max(h.qstart) - h.qstart.min(h.qend) + 1)
                .max()
                .unwrap_or(0);
            let mut kept: Vec<((u32, u32), (u32, u32))> = vec![];
            let mut total = 0;
            for hit in hits {
                let q = span(hit.qstart, hit.qend);
                let s = span(hit.sstart, hit.send);
                if !kept.iter().all(|&(kq, ks)| (q.0 < kq.0) == (s.0 < ks.0)) {
                    continue;
                }
                let shared: u32 = kept
                    .iter()
                    .map(|&(kq, ks)| overlap(q, kq).max(overlap(s, ks)))
                    .sum();
                let adds = (q.1 - q.0 + 1).saturating_sub(shared);
                if adds > 0 {
                    total += adds;
                    kept.push((q, s));
                }
            }
            (pair, total.max(longest))
        })
        .collect()
}

/// Whether a row of a SIFTS accession passes: at `MIN_SIFTS_IDENTITY` or
/// more itself, and its accession's rows (`coverage`, from `sifts_coverage`)
/// covering `MIN_SIFTS_COVERAGE` of the chain together. A row whose
/// accession is not in `coverage` (not listed by SIFTS for this chain, or
/// no row at 70%) does not.
fn passes_sifts(
    hit: &PolymerBlastHit,
    coverage: &HashMap<(String, String), u32>,
) -> bool {
    let pair = (hit.qaccver.clone(), accession(&hit.saccver));
    hit.qlen > 0
        && identity(hit) >= MIN_SIFTS_IDENTITY
        && coverage
            .get(&pair)
            .is_some_and(|&n| n as f64 / hit.qlen as f64 >= MIN_SIFTS_COVERAGE)
}

/// One `blastp` of `members` (FASTA IDs `p<i>`) against the sequences of
/// `accessions` (FASTA IDs the accessions), their shared candidates
fn compare(
    dir: &Path,
    n: usize,
    queries: &[PolymerQuery],
    members: &[usize],
    accessions: &[String],
    entries: &Entries,
) -> Result<Vec<PolymerBlastHit>> {
    let subjects: String = accessions
        .iter()
        .filter_map(|acc| {
            let entry = entries.get(acc)?.as_ref().ok()?;
            (!entry.sequence.is_empty())
                .then(|| format!(">{acc}\n{}\n", entry.sequence))
        })
        .collect();
    if subjects.is_empty() {
        return Ok(vec![]);
    }
    let query = dir.join(format!("{n}.query.fa"));
    let subject = dir.join(format!("{n}.subject.fa"));
    let results = dir.join(format!("{n}.tsv"));
    let text: String = members
        .iter()
        .map(|&i| format!(">p{i}\n{}\n", queries[i].sequence))
        .collect();
    fs::write(&query, text).map_err(|e| anyhow!("{}: {e}", query.display()))?;
    fs::write(&subject, subjects).map_err(|e| anyhow!("{}: {e}", subject.display()))?;
    run_blastp_subject(&query, &subject, MAX_TARGET_SEQS, BLAST_OUTFMT, &results)?;
    read_hits(&results)
}

// --------------------------------------------------
/// Each accession's UniProt entry, fetched once, several at a time, with
/// two retries; or why it could not be had. With `cache`, an entry already
/// there (`<accession>.json`: when it was fetched, and UniProt's JSON) is
/// read instead of fetched, and a fetched one is saved, so a later run
/// sees the same entries: a backfill's real run writes what its dry run
/// reported.
fn fetch_entries<'a>(
    accessions: impl Iterator<Item = &'a str>,
    cache: Option<&Path>,
) -> Entries {
    let mut accessions: Vec<&str> = accessions.collect();
    accessions.sort_unstable();
    accessions.dedup();
    if accessions.is_empty() {
        return HashMap::new();
    }
    let fetch = |acc: &str| {
        if let Some(entry) = cache.and_then(|dir| read_cached(dir, acc)) {
            return Ok(entry);
        }
        let mut last = String::new();
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_secs(2 * attempt));
            }
            match get_uniprot_entry(acc) {
                Ok(entry) => {
                    if let Some(dir) = cache
                        && let Err(e) = write_cached(dir, &entry)
                    {
                        warn!("UniProt cache {}: {e}", dir.display());
                    }
                    return Ok(entry);
                }
                Err(e) => last = e.to_string(),
            }
        }
        Err(last)
    };
    let results: Vec<(String, std::result::Result<UniprotEntry, String>)> =
        in_pool(UNIPROT_FETCHES, || {
            accessions
                .par_iter()
                .map(|acc| (acc.to_string(), fetch(acc)))
                .collect()
        });
    info!(
        "{} UniProt entries{}",
        results.len(),
        cache.map_or(String::new(), |d| format!(" (cache {})", d.display()))
    );
    results.into_iter().collect()
}

/// `work` on a pool of `num_threads`, or on rayon's global pool when one
/// cannot be built
fn in_pool<T: Send>(num_threads: usize, work: impl FnOnce() -> T + Send) -> T {
    match rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
    {
        Ok(pool) => pool.install(work),
        Err(_) => work(),
    }
}

/// A cached entry: when it was fetched, and UniProt's JSON
#[derive(serde::Serialize, Deserialize)]
struct Cached {
    fetched_at: chrono::DateTime<chrono::Utc>,
    response: serde_json::Value,
}

fn cache_file(dir: &Path, accession: &str) -> std::path::PathBuf {
    dir.join(format!("{accession}.json"))
}

fn read_cached(dir: &Path, accession: &str) -> Option<UniprotEntry> {
    let text = fs::read_to_string(cache_file(dir, accession)).ok()?;
    let cached: Cached = serde_json::from_str(&text).ok()?;
    uniprot_entry_from_json(accession, cached.response, cached.fetched_at).ok()
}

/// Written to a temporary name and renamed, so a reader never sees half
fn write_cached(dir: &Path, entry: &UniprotEntry) -> Result<()> {
    let (Some(response), Some(fetched_at)) = (&entry.response, entry.fetched_at) else {
        return Ok(());
    };
    let cached = Cached {
        fetched_at,
        response: response.clone(),
    };
    write_atomically(
        &cache_file(dir, &entry.uniprot_id),
        &serde_json::to_vec(&cached)?,
    )
}

fn write_atomically(file: &Path, contents: &[u8]) -> Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(format!(".{}", std::process::id()));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, file)?;
    Ok(())
}

// --------------------------------------------------
/// For each tie (its accessions in `best_hits`'s order before UniRef), the
/// UniRef100 representative of its first entries, until one names an entry
/// of the tie, asking at most `UNIREF_TRIES`. Identical sequences are one
/// UniRef100 cluster, so the first usually answers. Accession ->
/// representative; an accession whose lookup failed or found no cluster
/// (an isoform) is left out, and its tie is broken as before.
pub fn representatives(
    ties: &[Vec<String>],
    cache: Option<&Path>,
) -> HashMap<String, String> {
    if ties.is_empty() {
        return HashMap::new();
    }
    let walk = |tie: &Vec<String>| {
        let mut found = vec![];
        for acc in tie.iter().take(UNIREF_TRIES) {
            if let Some(rep) = uniref100_representative(acc, cache) {
                let done = tie.contains(&rep);
                found.push((acc.clone(), rep));
                if done {
                    break;
                }
            }
        }
        found
    };
    in_pool(UNIPROT_FETCHES, || {
        ties.par_iter().flat_map(walk).collect::<Vec<_>>()
    })
    .into_iter()
    .collect()
}

/// The representative of the UniRef100 cluster holding `accession`, from
/// UniProt (`representativeMember`). None when it has no cluster, or the
/// call failed (logged). With `cache`, the answer is kept as
/// `<accession>.uniref100` (empty for no cluster), so a later run breaks
/// the tie the same way.
fn uniref100_representative(accession: &str, cache: Option<&Path>) -> Option<String> {
    let file = cache.map(|dir| dir.join(format!("{accession}.uniref100")));
    if let Some(file) = &file
        && let Ok(text) = fs::read_to_string(file)
    {
        let rep = text.trim();
        return (!rep.is_empty()).then(|| rep.to_string());
    }
    let url = format!(
        "https://rest.uniprot.org/uniref/search?query=uniprot_id:{accession}+AND+identity:1.0"
    );
    let got = (|| -> Result<Option<String>> {
        let resp = reqwest::blocking::get(&url)?;
        if !resp.status().is_success() {
            bail!("{}", resp.status());
        }
        let json: serde_json::Value = resp.json()?;
        Ok(json
            .pointer("/results/0/representativeMember/accessions/0")
            .and_then(|acc| acc.as_str())
            .map(str::to_string))
    })();
    match got {
        Ok(rep) => {
            if let Some(file) = &file
                && let Err(e) =
                    write_atomically(file, rep.as_deref().unwrap_or("").as_bytes())
            {
                warn!("UniRef cache {}: {e}", file.display());
            }
            rep
        }
        Err(e) => {
            debug!("UniRef100 for {accession}: {e}; tie broken without it");
            None
        }
    }
}

fn none() -> PolymerLookup {
    PolymerLookup {
        match_method: NONE.to_string(),
        hit: None,
    }
}

fn lookup(candidate: &Candidate, hit: PolymerHit) -> PolymerLookup {
    PolymerLookup {
        match_method: candidate.match_method.to_string(),
        hit: Some(hit),
    }
}

fn not_recorded(candidate: &Candidate, e: &str) -> String {
    format!(
        "A chain's UniProt reference ({}, by {}) was not recorded: {e}",
        candidate.accession, candidate.match_method
    )
}

// --------------------------------------------------
/// The hit for a candidate whose UniProt entry is in hand. None when a range
/// does not fit the sequences.
pub fn make_hit(
    candidate: &Candidate,
    sequence: &str,
    uniprot: &UniprotEntry,
) -> Option<PolymerHit> {
    let (qs, qe) = (candidate.query_start, candidate.query_end);
    let (rs, re) = (candidate.reference_start, candidate.reference_end);
    if qs < 1 || qe < qs || qe as usize > sequence.len() {
        return None;
    }
    if rs < 1 || re < rs || re as usize > uniprot.sequence.len() {
        return None;
    }
    Some(PolymerHit {
        uniprot: uniprot.clone(),
        query_start: qs,
        query_end: qe,
        reference_start: rs,
        reference_end: re,
        identity: candidate.identity,
    })
}

// --------------------------------------------------
/// The UniProt entries SIFTS lists for each of `pdb_ids` (lower case), any
/// chain, read in one pass: by PDB ID, sorted, each once. An ID SIFTS does
/// not list is absent.
pub fn sifts_accessions(
    sifts: &Path,
    pdb_ids: &HashSet<String>,
) -> Result<HashMap<String, Vec<String>>> {
    let mut found: HashMap<String, Vec<String>> = HashMap::new();
    if pdb_ids.is_empty() {
        return Ok(found);
    }
    let file = File::open(sifts).map_err(|e| anyhow!("{}: {e}", sifts.display()))?;

    // PDB CHAIN SP_PRIMARY RES_BEG RES_END PDB_BEG PDB_END SP_BEG SP_END
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| anyhow!("{}: {e}", sifts.display()))?;
        if line.starts_with('#') || line.starts_with("PDB\t") {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            continue;
        }
        let pdb = fields[0].to_lowercase();
        let acc = fields[2].trim();
        if pdb_ids.contains(&pdb) && !acc.is_empty() {
            found.entry(pdb).or_default().push(acc.to_string());
        }
    }
    for accs in found.values_mut() {
        accs.sort();
        accs.dedup();
    }
    Ok(found)
}

// --------------------------------------------------
/// Step 3: BLAST `queries` (FASTA ID, sequence) against Swiss-Prot, then
/// those with no passing hit against TrEMBL. Returns the best passing hit by
/// FASTA ID. A Swiss-Prot hit that passes is never weighed against TrEMBL.
pub fn blast_candidates(
    queries: &[(String, &str)],
    blast_dir: &Path,
    processed_dir: &Path,
    num_threads: usize,
    uniprot_cache: Option<&Path>,
) -> Result<HashMap<String, Candidate>> {
    if !blast_dir.is_dir() {
        return Err(anyhow!(r#"Invalid BLAST dir "{}""#, blast_dir.display()));
    }
    let mut found = HashMap::new();
    for db in [UniprotDb::Swissprot, UniprotDb::Trembl] {
        let remaining: Vec<&(String, &str)> = queries
            .iter()
            .filter(|(id, _)| !found.contains_key(id))
            .collect();
        if remaining.is_empty() {
            break;
        }
        let name = db.to_string().to_lowercase();
        let fasta = processed_dir.join(format!("polymers.{name}.fa"));
        let results = processed_dir.join(format!("blast.polymers.{name}.tsv"));
        if file_exists(&results) {
            debug!("{db} polymer BLAST results exist ({})", results.display());
        } else {
            let text: String = remaining
                .iter()
                .map(|(id, seq)| format!(">{id}\n{seq}\n"))
                .collect();
            fs::write(&fasta, text).map_err(|e| anyhow!("{}: {e}", fasta.display()))?;
            let (blast_db, _) = blast_db(blast_dir, &db);
            run_blastp(
                &fasta,
                &blast_db,
                MAX_TARGET_SEQS,
                BLAST_OUTFMT,
                &results,
                num_threads,
            )?;
        }
        let rows = read_hits(&results)?;
        let representatives = representatives(&top_ties(&rows), uniprot_cache);
        for (id, candidate) in best_hits(&rows, &representatives) {
            if remaining.iter().any(|(r, _)| *r == id) {
                found.insert(id, candidate);
            }
        }
    }
    Ok(found)
}

/// The rows of a `BLAST_OUTFMT` file
pub fn read_hits(results: &Path) -> Result<Vec<PolymerBlastHit>> {
    if !file_exists(results) {
        return Ok(vec![]);
    }
    let file =
        File::open(results).map_err(|e| anyhow!("{}: {e}", results.display()))?;
    csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .from_reader(BufReader::new(file))
        .deserialize()
        .map(|row| row.map_err(|e| anyhow!("{}: {e}", results.display())))
        .collect()
}

/// Our residues inside a hit's alignment
fn covered(hit: &PolymerBlastHit) -> u32 {
    hit.qend.max(hit.qstart) - hit.qstart.min(hit.qend) + 1
}

/// The percent of our aligned residues that are identical. BLAST's `pident`
/// divides by the alignment's length, which counts a residue our chain
/// lacks (a gap in ours: an unresolved loop) as a difference; this does not.
/// A residue of ours that UniProt lacks still counts against it.
pub fn identity(hit: &PolymerBlastHit) -> f64 {
    100.0 * hit.nident as f64 / covered(hit) as f64
}

/// Whether a BLAST hit is close enough to be the chain's reference
pub fn passes(hit: &PolymerBlastHit) -> bool {
    hit.qlen > 0
        && identity(hit) >= MIN_IDENTITY
        && hit.mismatch <= MAX_MISMATCHES
        && covered(hit) as f64 / hit.qlen as f64 >= MIN_COVERAGE
}

/// The accession in `sp|P04585|POL_HV1H2` or `tr|A0A...|...`
pub fn accession(saccver: &str) -> String {
    let re = Regex::new(r"^(?:sp|tr)[|]([^|]+)[|]").expect("valid regex");
    re.captures(saccver)
        .and_then(|c| c.get(1))
        .map_or(saccver, |m| m.as_str())
        .to_string()
}

/// Each query's hits that `pass`, in order: the top bit score, then a
/// canonical entry over an isoform (`P12345-2`), then the lowest accession
fn ranked<'a>(
    hits: &'a [PolymerBlastHit],
    pass: &dyn Fn(&PolymerBlastHit) -> bool,
) -> HashMap<&'a str, Vec<&'a PolymerBlastHit>> {
    let mut by_query: HashMap<&str, Vec<&PolymerBlastHit>> = HashMap::new();
    for hit in hits.iter().filter(|h| pass(h)) {
        by_query.entry(hit.qaccver.as_str()).or_default().push(hit);
    }
    for hits in by_query.values_mut() {
        hits.sort_by(|a, b| {
            let (acc_a, acc_b) = (accession(&a.saccver), accession(&b.saccver));
            b.bitscore
                .total_cmp(&a.bitscore)
                .then_with(|| acc_a.contains('-').cmp(&acc_b.contains('-')))
                .then_with(|| acc_a.cmp(&acc_b))
        });
    }
    by_query
}

/// The distinct accessions tied at a query's top bit score, in `ranked`
/// order
fn tied(hits: &[&PolymerBlastHit]) -> Vec<String> {
    let mut accessions: Vec<String> = vec![];
    for hit in hits.iter().take_while(|h| h.bitscore == hits[0].bitscore) {
        let acc = accession(&hit.saccver);
        if !accessions.contains(&acc) {
            accessions.push(acc);
        }
    }
    accessions
}

/// Every query's tie at the top bit score, when it has one: what
/// `representatives` is asked about
pub fn top_ties(hits: &[PolymerBlastHit]) -> Vec<Vec<String>> {
    top_ties_by(hits, &passes)
}

/// `top_ties` with `pass` in place of `passes`
fn top_ties_by(
    hits: &[PolymerBlastHit],
    pass: &dyn Fn(&PolymerBlastHit) -> bool,
) -> Vec<Vec<String>> {
    ranked(hits, pass)
        .values()
        .map(|hits| tied(hits))
        .filter(|tie| tie.len() > 1)
        .collect()
}

/// The best passing hit for each query: the top bit score, then the UniRef100
/// representative (`representatives`: accession -> its representative) when
/// it is one of the tied entries, then `ranked`'s order
pub fn best_hits(
    hits: &[PolymerBlastHit],
    representatives: &HashMap<String, String>,
) -> HashMap<String, Candidate> {
    best_hits_by(hits, representatives, &passes)
}

/// `best_hits` with `pass` in place of `passes`
fn best_hits_by(
    hits: &[PolymerBlastHit],
    representatives: &HashMap<String, String>,
    pass: &dyn Fn(&PolymerBlastHit) -> bool,
) -> HashMap<String, Candidate> {
    let mut best = HashMap::new();
    for (query, hits) in ranked(hits, pass) {
        let tie = tied(&hits);
        let is_representative = |acc: &str| {
            tie.iter()
                .any(|t| representatives.get(t).is_some_and(|r| r == acc))
        };
        let top = hits
            .iter()
            .take_while(|h| h.bitscore == hits[0].bitscore)
            .find(|h| is_representative(&accession(&h.saccver)))
            .unwrap_or(&hits[0]);
        if tie.len() > 1 {
            debug!(
                "{query}: bit score {} ties {}; took {}",
                top.bitscore,
                tie.join(", "),
                accession(&top.saccver)
            );
        }
        best.insert(
            query.to_string(),
            Candidate {
                match_method: ALIGNED,
                accession: accession(&top.saccver),
                query_start: top.qstart.min(top.qend) as i32,
                query_end: top.qstart.max(top.qend) as i32,
                reference_start: top.sstart.min(top.send) as i32,
                reference_end: top.sstart.max(top.send) as i32,
                identity: identity(top),
            },
        );
    }
    best
}

// --------------------------------------------------
/// A warning when the declared PDB ID looks wrong: SIFTS maps the entry to
/// UniProt (`listed`), but no protein polymer of the simulation is related
/// to any of those entries (`related_to_pdb`). `related` holds, for each of
/// the simulation's protein polymers long enough to look up, whether it is
/// related, by its OWN declared IDs (a backfill must not use a reference
/// another simulation decided), or `None` when it was not looked up (e.g. a
/// UniProt fetch failed). No warning then, nor when the entry is not in
/// SIFTS (nothing to compare). 21 PLAS-20K simulations declare a PDB entry
/// of another protein, e.g. 84136: 4tq8 (transthyretin) with epoxide
/// hydrolase 2 (2026-10-15). A warning, not an error: the structure is what
/// was simulated, and the PDB ID is metadata to correct. It does not see a
/// system with one right protein and one foreign (2wva, 5hi4, 5km5, 5v03).
/// It also fires when only an unmapped part of the entry was simulated: 7
/// of 3,000 random simulations (a Fab of 3u0t, 8tmz, 6uud; 6okd's peptide;
/// 3cf6's Epac2, where SIFTS maps only Rap1b; 1lbk's GST chimera).
pub fn declared_pdb_not_matched(
    pdb_id: &str,
    listed: &[String],
    related: &[Option<bool>],
) -> Option<String> {
    if listed.is_empty() || !none_related(related) {
        return None;
    }
    Some(format!(
        r#"PDB ID "{pdb_id}" was declared, but no chain matches a UniProt entry that PDBe maps to it ({}). Check that it is the entry that was simulated. This is expected when only a part PDBe does not map was simulated, such as an antibody or a peptide."#,
        listed.join(", ")
    ))
}

/// For each protein polymer long enough to look up (`MIN_RESIDUES`), whether
/// it is related to its declared PDB entry (`related`, from
/// `lookup_polymers`), or `None` when it was not looked up: what
/// `declared_pdb_not_matched` takes when each polymer was looked up for this
/// simulation, as in an import
fn related_flags(
    queries: &[PolymerQuery],
    lookups: &[Option<PolymerLookup>],
    related: &HashSet<usize>,
) -> Vec<Option<bool>> {
    queries
        .iter()
        .enumerate()
        .filter(|(_, q)| q.polymer_type == "protein" && q.num_residues >= MIN_RESIDUES)
        .map(|(i, _)| lookups[i].as_ref().map(|_| related.contains(&i)))
        .collect()
}

/// Every protein polymer was looked up, and none is related to the declared
/// PDB entry
fn none_related(related: &[Option<bool>]) -> bool {
    !related.is_empty() && related.iter().all(|r| *r == Some(false))
}

// --------------------------------------------------
/// A warning for each declared accession that is no chain's reference, when
/// some chain has one. The link from the TOML is kept regardless (item 5).
pub fn declared_not_referenced(
    declared: &[String],
    chains: &[ImportChain],
) -> Vec<String> {
    let mut references: Vec<&str> = chains
        .iter()
        .filter_map(|c| c.reference.as_ref()?.hit.as_ref())
        .map(|h| h.uniprot.uniprot_id.as_str())
        .collect();
    references.sort();
    references.dedup();
    if references.is_empty() {
        return vec![];
    }
    let base = |acc: &str| acc.split('-').next().unwrap_or(acc).to_uppercase();
    declared
        .iter()
        .filter(|d| !references.iter().any(|r| base(r) == base(d)))
        .map(|d| {
            format!(
                r#"UniProt ID "{d}" was declared, but no chain's residues map to it (the chains map to {})"#,
                references.join(", ")
            )
        })
        .collect()
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const PROTEASE: &str = "PQITLWKRPLVTIKIGGQLKEALLDTGADDTVIEEMSLPGRWKPKMIGGIGGFIKVRQYDQIIIEICGHKAIGTVLVGPTPVNIIGRNLLTQIGCTLNF";

    /// Calmodulin's first 40 residues, a second protein for a fusion
    const CALM40: &str = "MADQLTEEQIAEFKEAFSLFDKDGDGTITTKELGTVMRSL";

    fn blast_row(
        q: &str,
        s: &str,
        pident: f64,
        mismatch: u32,
        qse: (u32, u32),
        qlen: u32,
        bits: f64,
    ) -> PolymerBlastHit {
        let length = qse.1 - qse.0 + 1;
        PolymerBlastHit {
            qaccver: q.into(),
            saccver: s.into(),
            pident,
            length,
            mismatch,
            gapopen: 0,
            qstart: qse.0,
            qend: qse.1,
            sstart: 10,
            send: 10 + qse.1 - qse.0,
            evalue: 1e-50,
            bitscore: bits,
            qlen,
            slen: 500,
            nident: (pident * length as f64 / 100.0).round() as u32,
        }
    }

    fn chain(order: u32, polymer_type: &str, sequence: &str) -> ImportChain {
        ImportChain {
            chain_order: order,
            chain_label: "A".into(),
            source: "structure".into(),
            polymer_type: polymer_type.into(),
            sequence: sequence.into(),
            residues: sequence.chars().map(|c| c.to_string()).collect(),
            first_residue: Some(1),
            last_residue: Some(sequence.len() as i32),
            n_terminal_cap: None,
            c_terminal_cap: None,
            breaks: vec![],
            reference: None,
        }
    }

    fn query(
        sequence: &str,
        pdb_id: Option<&str>,
        uniprot_ids: &[&str],
    ) -> PolymerQuery {
        PolymerQuery {
            polymer_type: "protein".into(),
            num_residues: sequence.len(),
            sequence: sequence.into(),
            pdb_id: pdb_id.map(str::to_string),
            uniprot_ids: uniprot_ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// The tests that run `blastp` skip without it
    fn has_blastp() -> bool {
        let found = which::which("blastp").is_ok();
        if !found {
            eprintln!("skipping: no blastp on PATH");
        }
        found
    }

    /// A UniProt cache holding `entries` (accession, sequence), so nothing is
    /// fetched
    fn cache_with(dir: &Path, entries: &[(&str, &str)]) {
        for (acc, sequence) in entries {
            let raw = serde_json::json!({
                "proteinDescription": {"recommendedName": {"fullName": {"value": acc}}},
                "sequence": {"value": sequence},
                "entryAudit": {"entryVersion": 1}
            });
            let entry = uniprot_entry_from_json(acc, raw, chrono::Utc::now()).unwrap();
            write_cached(dir, &entry).unwrap();
        }
    }

    /// A SIFTS file of (PDB ID, chain, accession) under `blast_dir`
    fn sifts_with(blast_dir: &Path, rows: &[(&str, &str, &str)]) -> std::path::PathBuf {
        let sifts = blast_dir.join(PDB_SIFTS);
        fs::create_dir_all(sifts.parent().unwrap()).unwrap();
        let mut text = String::from(
            "# 2026/10/04 - 08:26 | PDB: 40.26 | UniProt: 2026.03\n\
             PDB\tCHAIN\tSP_PRIMARY\tRES_BEG\tRES_END\tPDB_BEG\tPDB_END\tSP_BEG\tSP_END\n",
        );
        for (pdb, chain, acc) in rows {
            text.push_str(&format!("{pdb}\t{chain}\t{acc}\t1\t99\t1\t99\t1\t99\n"));
        }
        fs::write(&sifts, text).unwrap();
        sifts
    }

    /// Step 2 alone, for `queries`, with entries from `cache`
    fn declared(
        queries: &[PolymerQuery],
        sifts: &Path,
        work: &Path,
        cache: &Path,
    ) -> HashMap<usize, Declared> {
        let wanted: Vec<usize> = (0..queries.len()).collect();
        let (got, _, warnings, _) =
            declared_matches(queries, &wanted, sifts, work, Some(cache)).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        got
    }

    fn matched(got: &HashMap<usize, Declared>, i: usize) -> &Candidate {
        match &got[&i] {
            Declared::Match(c) => c,
            other => panic!("polymer {i}: {other:?}"),
        }
    }

    /// SIFTS lists an entry's accessions over all its chains, each once, and
    /// leaves out the entries not asked for
    #[test]
    fn sifts_accessions_by_entry() {
        let dir = tempdir().unwrap();
        let sifts = sifts_with(
            dir.path(),
            &[
                ("101m", "A", "P02185"),
                ("1a30", "A", "P04585"),
                ("1a30", "B", "P04585"),
                ("3uon", "A", "P08172"),
                ("3uon", "A", "P00720"),
            ],
        );
        let ids: HashSet<String> = ["1a30", "3uon", "9xyz"].map(String::from).into();
        let got = sifts_accessions(&sifts, &ids).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got["1a30"], vec!["P04585"]);
        assert_eq!(got["3uon"], vec!["P00720", "P08172"]);
    }

    /// SOL3's shape (MDR00105204): the declared entry and another strain's
    /// have the same sequence. The declared one is taken, by `declared`, and
    /// no search runs (there is no search database here, so one would fail).
    #[test]
    fn declared_uniprot_id_is_taken_without_a_search() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P38858", PROTEASE), ("A6ZT71", PROTEASE)]);
        sifts_with(dir.path(), &[]);
        let queries = vec![query(PROTEASE, None, &["p38858"])];
        let (lookups, warnings, _) =
            lookup_polymers(&queries, dir.path(), dir.path(), 1, Some(&cache)).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let lookup = lookups[0].as_ref().unwrap();
        assert_eq!(lookup.match_method, DECLARED);
        let hit = lookup.hit.as_ref().unwrap();
        assert_eq!(hit.uniprot.uniprot_id, "P38858");
        assert_eq!((hit.query_start, hit.query_end), (1, 99));
        assert_eq!((hit.reference_start, hit.reference_end), (1, 99));
        assert_eq!(hit.identity, 100.0);
    }

    /// The declared PDB entry's SIFTS accession is `pdb`, and so is a
    /// declared UniProt ID that SIFTS also lists; of two that pass with the
    /// same score, the UniRef100 representative is taken
    #[test]
    fn sifts_accessions_are_pdb() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P04585", PROTEASE), ("Q99999", PROTEASE)]);
        fs::write(cache.join("P04585.uniref100"), "Q99999").unwrap();
        let sifts = sifts_with(dir.path(), &[("1a30", "A", "P04585")]);
        let queries = vec![
            query(PROTEASE, Some("1A30"), &[]),
            query(PROTEASE, Some("1a30"), &["P04585"]),
            query(PROTEASE, Some("1a30"), &["Q99999"]),
        ];
        let got = declared(&queries, &sifts, dir.path(), &cache);
        assert_eq!(matched(&got, 0).accession, "P04585");
        assert_eq!(matched(&got, 0).match_method, PDB);
        assert_eq!(matched(&got, 1).match_method, PDB);
        assert_eq!(matched(&got, 2).accession, "Q99999");
        assert_eq!(matched(&got, 2).match_method, DECLARED);
    }

    /// A mutant with 2 mismatches passes its declared entry; with 5 it does
    /// not, and goes on to the search. A chain that declares nothing has no
    /// step 2 at all.
    #[test]
    fn mutants_and_the_declared_check() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P04585", PROTEASE)]);
        let sifts = sifts_with(dir.path(), &[]);
        let mutate = |positions: &[usize]| -> String {
            PROTEASE
                .chars()
                .enumerate()
                .map(|(i, c)| match (positions.contains(&i), c) {
                    (true, 'A') => 'S',
                    (true, _) => 'A',
                    (false, c) => c,
                })
                .collect()
        };
        let queries = vec![
            query(&mutate(&[20, 60]), None, &["P04585"]),
            query(&mutate(&[10, 30, 50, 70, 90]), None, &["P04585"]),
            query(PROTEASE, None, &[]),
        ];
        let got = declared(&queries, &sifts, dir.path(), &cache);
        let two = matched(&got, 0);
        assert_eq!(two.accession, "P04585");
        assert!((two.identity - 100.0 * 97.0 / 99.0).abs() < 1e-9);
        assert_eq!(got[&1], Declared::NoMatch);
        assert!(!got.contains_key(&2));
    }

    /// A SIFTS accession is PDBe's assignment: taken whenever BLAST aligns
    /// it over half the chain, so a mutant with 5 mismatches (3gcp's case)
    /// keeps it. The same mutant declared only by UniProt ID still fails,
    /// and so does a SIFTS accession covering under half (40 of 139), or
    /// one under 70% identical (every third residue changed, 66/99).
    #[test]
    fn sifts_accessions_skip_the_pass_rule() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P04585", PROTEASE), ("P0DP23", CALM40)]);
        let sifts = sifts_with(
            dir.path(),
            &[("1a30", "A", "P04585"), ("9yyy", "A", "P0DP23")],
        );
        let mutant: String = PROTEASE
            .chars()
            .enumerate()
            .map(|(i, c)| match ([10, 30, 50, 70, 90].contains(&i), c) {
                (true, 'A') => 'S',
                (true, _) => 'A',
                (false, c) => c,
            })
            .collect();
        let distant: String = PROTEASE
            .chars()
            .enumerate()
            .map(|(i, c)| match (i % 3 == 0, c) {
                (true, 'A') => 'S',
                (true, _) => 'A',
                (false, c) => c,
            })
            .collect();
        let queries = vec![
            query(&mutant, Some("1a30"), &[]),
            query(&mutant, None, &["P04585"]),
            query(&format!("{CALM40}{PROTEASE}"), Some("9yyy"), &[]),
            query(&distant, Some("1a30"), &[]),
        ];
        let got = declared(&queries, &sifts, dir.path(), &cache);
        let five = matched(&got, 0);
        assert_eq!(
            (five.accession.as_str(), five.match_method),
            ("P04585", PDB)
        );
        assert_eq!(got[&1], Declared::NoMatch);
        assert_eq!(got[&2], Declared::NoMatch);
        assert_eq!(got[&3], Declared::NoMatch);
    }

    /// Ends trimmed, and a loop of 10 missing (which BLAST's `pident` would
    /// put at 89.9%): BLAST's ranges are stored, and the loop costs no
    /// identity
    #[test]
    fn trimmed_and_looped_chains_take_blasts_ranges() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        let tagged = format!("MGSHHHHHH{PROTEASE}");
        cache_with(&cache, &[("P04585", &tagged)]);
        let sifts = sifts_with(dir.path(), &[]);
        let looped = format!("{}{}", &PROTEASE[..40], &PROTEASE[50..]);
        let queries = vec![
            query(&PROTEASE[5..95], None, &["P04585"]),
            query(&looped, None, &["P04585"]),
        ];
        let got = declared(&queries, &sifts, dir.path(), &cache);
        let trimmed = matched(&got, 0);
        assert_eq!((trimmed.query_start, trimmed.query_end), (1, 90));
        assert_eq!((trimmed.reference_start, trimmed.reference_end), (15, 104));
        let loop_ = matched(&got, 1);
        assert_eq!((loop_.query_start, loop_.query_end), (1, 89));
        assert_eq!((loop_.reference_start, loop_.reference_end), (10, 108));
        assert_eq!(loop_.identity, 100.0);
    }

    /// A repeat protein: BLAST's range is taken as it is, and the same input
    /// gives the same answer
    #[test]
    fn a_repeat_protein_is_deterministic() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        let unit = "WKRPLVTIKIGGQLKEALLD";
        cache_with(&cache, &[("Q11111", &format!("M{}", unit.repeat(5)))]);
        let sifts = sifts_with(dir.path(), &[]);
        let queries = vec![query(&unit.repeat(2), None, &["Q11111"])];
        let first = matched(&declared(&queries, &sifts, dir.path(), &cache), 0).clone();
        assert_eq!((first.query_start, first.query_end), (1, 40));
        for _ in 0..3 {
            let again = declared(&queries, &sifts, dir.path(), &cache);
            assert_eq!(matched(&again, 0), &first);
        }
    }

    /// A fusion (the protease, then 40 of calmodulin) whose PDB entry SIFTS
    /// maps to both partners: neither covers 90% of the chain, but SIFTS
    /// accessions skip the pass rule, so the partner with the top bit score
    /// is taken, as 0.6.0 took SIFTS's largest segment (D3)
    #[test]
    fn a_fusion_takes_the_best_sifts_partner() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P04585", PROTEASE), ("P0DP23", CALM40)]);
        let sifts = sifts_with(
            dir.path(),
            &[("9zzz", "A", "P04585"), ("9zzz", "A", "P0DP23")],
        );
        let queries = vec![query(&format!("{PROTEASE}{CALM40}"), Some("9zzz"), &[])];
        let got = declared(&queries, &sifts, dir.path(), &cache);
        let fused = matched(&got, 0);
        assert_eq!(
            (fused.accession.as_str(), fused.match_method),
            ("P04585", PDB)
        );
        assert_eq!((fused.query_start, fused.query_end), (1, 99));
    }

    /// The protease with 80 residues inserted after its 50th (3odu's CXCR4
    /// with T4 lysozyme in a loop): BLAST gives two rows, 1-50 and 129-179,
    /// each 28% of the chain, sharing two residues of the entry at the
    /// insert. Together they are 55%, so the SIFTS accession is taken, with
    /// the range of the row with the top bit score. Declared only by UniProt
    /// ID, it still fails.
    #[test]
    fn a_sifts_accession_split_by_an_insert_is_taken() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P04585", PROTEASE)]);
        let sifts = sifts_with(dir.path(), &[("9iii", "A", "P04585")]);
        let inserted =
            format!("{}{CALM40}{CALM40}{}", &PROTEASE[..50], &PROTEASE[50..]);
        let queries = vec![
            query(&inserted, Some("9iii"), &[]),
            query(&inserted, None, &["P04585"]),
        ];
        let got = declared(&queries, &sifts, dir.path(), &cache);
        let split = matched(&got, 0);
        assert_eq!(
            (split.accession.as_str(), split.match_method),
            ("P04585", PDB)
        );
        assert_eq!(split.identity, 100.0);
        assert!(
            [(1, 50), (129, 179)].contains(&(split.query_start, split.query_end)),
            "{split:?}"
        );
        assert_eq!(got[&1], Declared::NoMatch);
    }

    /// Rows count together only in the same order in the chain and the
    /// entry, and only for the residues they add: a repeat's second row on
    /// the same residues of the entry adds nothing, and a row under 70%
    /// identity is not counted
    #[test]
    fn sifts_coverage_counts_each_residue_once() {
        let row = |qse: (u32, u32), sse: (u32, u32), pident: f64, bits: f64| {
            PolymerBlastHit {
                sstart: sse.0,
                send: sse.1,
                ..blast_row("p0", "P04585", pident, 0, qse, 200, bits)
            }
        };
        let listed: HashSet<(String, String)> =
            [("p0".to_string(), "P04585".to_string())].into();
        let pair = ("p0".to_string(), "P04585".to_string());
        let cover = |rows: &[PolymerBlastHit]| sifts_coverage(rows, &listed)[&pair];

        // An insert: 1-50 and 129-179 on the entry's 1-50 and 49-99
        let insert = [
            row((1, 50), (1, 50), 100.0, 100.0),
            row((129, 179), (49, 99), 100.0, 99.0),
        ];
        assert_eq!(cover(&insert), 99);

        // A repeat: two units of ours on the same 40 residues of the entry
        let repeat = [
            row((1, 40), (10, 49), 100.0, 80.0),
            row((41, 80), (10, 49), 100.0, 79.0),
        ];
        assert_eq!(cover(&repeat), 40);

        // Out of order: our second half on the entry's first half
        let swapped = [
            row((1, 50), (51, 100), 100.0, 100.0),
            row((101, 150), (1, 50), 100.0, 99.0),
        ];
        assert_eq!(cover(&swapped), 50);

        // A row under 70% identity is not counted
        let distant = [
            row((1, 50), (1, 50), 100.0, 100.0),
            row((101, 150), (51, 100), 60.0, 40.0),
        ];
        assert_eq!(cover(&distant), 50);

        // Two inserts: three rows, all counted
        let three = [
            row((1, 40), (1, 40), 100.0, 80.0),
            row((61, 100), (41, 80), 100.0, 79.0),
            row((121, 160), (81, 120), 100.0, 78.0),
        ];
        assert_eq!(cover(&three), 120);

        // Taken by bit score, not file order: the full row first, so a
        // lower one out of order with it adds nothing (in file order, the
        // low row would be counted and the full one refused: 50)
        let low_first = [
            row((101, 150), (1, 50), 100.0, 60.0),
            row((1, 100), (1, 100), 100.0, 180.0),
        ];
        assert_eq!(cover(&low_first), 100);

        // A tethered dimer, both copies on the same residues of the entry
        // (3nxn, 202 residues): the higher-scoring row is the shorter, the
        // longer is out of order with it, and still counts alone
        let dimer = [
            row((1, 99), (491, 589), 92.9, 191.0),
            row((88, 202), (473, 589), 84.3, 186.0),
        ];
        assert_eq!(cover(&dimer), 115);

        // A pair SIFTS does not list is absent
        let other = [blast_row("p1", "P04585", 100.0, 0, (1, 99), 99, 180.0)];
        assert!(sifts_coverage(&other, &listed).is_empty());

        // Each listed pair is counted on its own rows, and a search
        // database's `sp|...|` name is read as its accession
        let listed2: HashSet<(String, String)> = [
            ("p0".to_string(), "P04585".to_string()),
            ("p0".to_string(), "P0DP23".to_string()),
        ]
        .into();
        let mixed = [
            row((1, 50), (1, 50), 100.0, 100.0),
            PolymerBlastHit {
                sstart: 1,
                send: 30,
                ..blast_row("p0", "sp|P0DP23|CALM_HUMAN", 100.0, 0, (60, 89), 200, 50.0)
            },
        ];
        let got = sifts_coverage(&mixed, &listed2);
        assert_eq!(got[&pair], 50);
        assert_eq!(got[&("p0".to_string(), "P0DP23".to_string())], 30);
    }

    /// A SIFTS row passes on its accession's coverage together, at half the
    /// chain or more, when the row itself is 70% identical
    #[test]
    fn passes_sifts_rule() {
        let pair = ("p0".to_string(), "P04585".to_string());
        let coverage = |n: u32| HashMap::from([(pair.clone(), n)]);
        let hit = blast_row("p0", "P04585", 100.0, 0, (1, 50), 200, 100.0);

        // The row is 25% of the chain; its accession's rows 50% or more
        assert!(passes_sifts(&hit, &coverage(100)));
        assert!(passes_sifts(&hit, &coverage(150)));
        assert!(!passes_sifts(&hit, &coverage(99)));

        // Under 70% itself, whatever the coverage
        let low = blast_row("p0", "P04585", 60.0, 0, (1, 50), 200, 100.0);
        assert!(!passes_sifts(&low, &coverage(200)));

        // Not in `coverage`: another accession, or another chain
        assert!(!passes_sifts(&hit, &HashMap::new()));
        let other = blast_row("p0", "P0DP23", 100.0, 0, (1, 50), 200, 100.0);
        assert!(!passes_sifts(&other, &coverage(200)));
        let chain = blast_row("p1", "P04585", 100.0, 0, (1, 50), 200, 100.0);
        assert!(!passes_sifts(&chain, &coverage(200)));

        // A row with no chain length never passes
        let empty = PolymerBlastHit {
            qlen: 0,
            ..hit.clone()
        };
        assert!(!passes_sifts(&empty, &coverage(200)));
    }

    /// A structure file with residues `(number, one-letter)` laid as a
    /// backbone (N, CA, C) along x, 3.8 A per residue, so each C is 1.33 A
    /// from the next N; `gap_before` adds that many angstroms before a
    /// residue, which breaks the bond there
    fn backbone_pdb(
        residues: &[(i32, char)],
        gap_before: Option<(i32, f64)>,
    ) -> String {
        const STANDARD: &[&str] = &[
            "ALA", "ARG", "ASN", "ASP", "CYS", "GLN", "GLU", "GLY", "HIS", "ILE",
            "LEU", "LYS", "MET", "PHE", "PRO", "SER", "THR", "TRP", "TYR", "VAL",
        ];
        let three = |c: char| {
            *STANDARD
                .iter()
                .find(|name| crate::sequence::one_letter(name) == Some(c))
                .unwrap()
        };
        let mut text = String::new();
        let (mut serial, mut x0) = (0, 0.0);
        for &(num, c) in residues {
            if let Some((before, gap)) = gap_before
                && before == num
            {
                x0 += gap;
            }
            for (name, dx) in [("N", 0.0), ("CA", 1.2), ("C", 2.47)] {
                serial += 1;
                text.push_str(&format!(
                    "ATOM  {serial:>5} {name:<4} {:>3} A{num:>4}    \
                     {:>8.3}{:>8.3}{:>8.3}  1.00  0.00           C  \n",
                    three(c),
                    x0 + dx,
                    0.0,
                    0.0
                ));
            }
            x0 += 3.8;
        }
        text.push_str("END\n");
        text
    }

    /// End to end from a structure file: the protease with residues 41-50
    /// unresolved (numbers 1-40, then 51-99, the backbone not bonded across)
    /// is one chain with a break; the same 89 residues closed into one bonded
    /// chain (4s0v's shape: the loop removed in the simulation) have none.
    /// BLAST does not read the breaks, so both match the declared entry over
    /// the whole of it.
    #[test]
    fn missing_loop_from_a_structure_file() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        cache_with(&cache, &[("P04585", PROTEASE)]);
        let sifts = sifts_with(dir.path(), &[("1a30", "A", "P04585")]);

        let looped: Vec<(i32, char)> = (1..=99)
            .zip(PROTEASE.chars())
            .filter(|(num, _)| !(41..=50).contains(num))
            .collect();
        let looped_seq: String = looped.iter().map(|(_, c)| c).collect();
        let closed: Vec<(i32, char)> = (1..).zip(looped_seq.chars()).collect();

        let mut queries = vec![];
        for (name, residues, gap, breaks) in [
            ("looped.pdb", &looped, Some((51, 30.0)), vec![40]),
            ("closed.pdb", &closed, None, vec![]),
        ] {
            let pdb = dir.path().join(name);
            fs::write(&pdb, backbone_pdb(residues, gap)).unwrap();
            let found = crate::sequence::chains_from_pdb(&pdb).unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].sequence, looped_seq);
            let (chains, _) = crate::process::place_chains(&found, None, &pdb).unwrap();
            assert_eq!(chains[0].breaks, breaks);
            queries.push(query(&chains[0].sequence, Some("1a30"), &[]));
        }

        let got = declared(&queries, &sifts, dir.path(), &cache);
        for i in 0..2 {
            let c = matched(&got, i);
            assert_eq!((c.accession.as_str(), c.match_method), ("P04585", PDB));
            assert_eq!((c.query_start, c.query_end), (1, 89));
            assert_eq!((c.reference_start, c.reference_end), (1, 99));
            assert_eq!(c.identity, 100.0);
        }
    }

    /// A range past the UniProt sequence gives no hit
    #[test]
    fn make_hit_checks_the_ranges() {
        let candidate = Candidate {
            match_method: ALIGNED,
            accession: "Q00000".into(),
            query_start: 1,
            query_end: 4,
            reference_start: 3,
            reference_end: 6,
            identity: 75.0,
        };
        let uniprot = UniprotEntry {
            uniprot_id: "Q00000".into(),
            name: "test".into(),
            sequence: "MMACDF".into(),
            ..Default::default()
        };
        let hit = make_hit(&candidate, "ACDE", &uniprot).unwrap();
        assert_eq!(hit.identity, 75.0);
        assert_eq!((hit.reference_start, hit.reference_end), (3, 6));

        let past = Candidate {
            reference_end: 7,
            ..candidate
        };
        assert_eq!(make_hit(&past, "ACDE", &uniprot), None);
    }

    /// An entry in the cache is read, not fetched (Q99999 does not exist,
    /// so a fetch would fail), and comes back as it was saved
    #[test]
    fn uniprot_cache_is_read_back() {
        let dir = tempdir().unwrap();
        let raw = serde_json::json!({
            "proteinDescription": {"recommendedName": {"fullName": {"value": "Cached protein"}}},
            "sequence": {"value": "MACDE"},
            "entryAudit": {"entryVersion": 7}
        });
        let fetched_at = chrono::Utc::now();
        let entry = uniprot_entry_from_json("Q99999", raw.clone(), fetched_at).unwrap();
        write_cached(dir.path(), &entry).unwrap();

        let got = fetch_entries(["Q99999"].into_iter(), Some(dir.path()));
        let back = got["Q99999"].as_ref().unwrap();
        assert_eq!(back, &entry);
        assert_eq!(back.entry_version, Some(7));
        assert_eq!(back.response, Some(raw));
        assert_eq!(back.fetched_at, Some(fetched_at));
    }

    /// SOL3's four entries tie in the search: the UniRef100 representative
    /// (P38858) is taken, not the lowest accession; without one (the lookup
    /// failed), the lowest
    #[test]
    fn search_tie_goes_to_the_representative() {
        let rows: Vec<PolymerBlastHit> = [
            "tr|B5VK90|B5VK90_YEAS6",
            "sp|P38858|SOL3_YEAST",
            "sp|A6ZT71|SOL3_YEAS7",
            "tr|B3LSS7|B3LSS7_YEAS1",
        ]
        .iter()
        .map(|s| blast_row("p0", s, 100.0, 0, (1, 249), 249, 502.0))
        .collect();
        assert_eq!(
            top_ties(&rows),
            vec![vec!["A6ZT71", "B3LSS7", "B5VK90", "P38858"]]
        );

        let reps: HashMap<String, String> =
            [("A6ZT71".to_string(), "P38858".to_string())].into();
        assert_eq!(best_hits(&rows, &reps)["p0"].accession, "P38858");
        assert_eq!(best_hits(&rows, &HashMap::new())["p0"].accession, "A6ZT71");

        // A representative outside the tie changes nothing
        let reps: HashMap<String, String> =
            [("A6ZT71".to_string(), "H0GHJ8".to_string())].into();
        assert_eq!(best_hits(&rows, &reps)["p0"].accession, "A6ZT71");
    }

    /// Representatives come from the cache: the first entry of a tie is
    /// asked, and the rest only when its answer is not in the tie. An empty
    /// answer (an isoform, no cluster) moves on to the next.
    #[test]
    fn representatives_from_the_cache() {
        let dir = tempdir().unwrap();
        for (acc, rep) in [("A6ZT71", "P38858"), ("P0DP23-2", ""), ("Q11111", "Q11111")]
        {
            fs::write(dir.path().join(format!("{acc}.uniref100")), rep).unwrap();
        }
        let ties = vec![
            vec!["A6ZT71".to_string(), "B3LSS7".into(), "P38858".into()],
            vec!["P0DP23-2".to_string(), "Q11111".into()],
        ];
        let got = representatives(&ties, Some(dir.path()));
        let want: HashMap<String, String> = [
            ("A6ZT71".to_string(), "P38858".to_string()),
            ("Q11111".to_string(), "Q11111".to_string()),
        ]
        .into();
        assert_eq!(got, want);
    }

    /// A mutant (one mismatch) passes; 1ktt's two chains read as one fail
    /// the 90% coverage floor; 94% identity fails; 4 mismatches fail; a gap
    /// in ours (a missing loop) costs no identity, but a residue of ours that
    /// UniProt lacks does
    #[test]
    fn blast_pass_rule() {
        assert!(passes(&blast_row(
            "p0",
            "sp|P1|X",
            99.0,
            1,
            (1, 100),
            100,
            200.0
        )));
        assert!(!passes(&blast_row(
            "p0",
            "sp|P1|X",
            100.0,
            0,
            (1, 160),
            320,
            300.0
        )));
        assert!(!passes(&blast_row(
            "p0",
            "sp|P1|X",
            94.0,
            2,
            (1, 100),
            100,
            200.0
        )));
        assert!(!passes(&blast_row(
            "p0",
            "sp|P1|X",
            96.0,
            4,
            (1, 100),
            100,
            200.0
        )));
        assert!(passes(&blast_row(
            "p0",
            "sp|P1|X",
            100.0,
            0,
            (6, 95),
            100,
            200.0
        )));

        // 89 of ours, all identical, aligned over 99 columns: 10 gaps in ours
        let looped = PolymerBlastHit {
            length: 99,
            pident: 89.899,
            gapopen: 1,
            ..blast_row("p0", "sp|P1|X", 100.0, 0, (1, 89), 89, 174.0)
        };
        assert_eq!(identity(&looped), 100.0);
        assert!(passes(&looped));
        // 99 of ours, 89 identical: 10 that UniProt lacks
        let inserted = PolymerBlastHit {
            length: 99,
            nident: 89,
            gapopen: 1,
            ..blast_row("p0", "sp|P1|X", 100.0, 0, (1, 99), 99, 174.0)
        };
        assert!(!passes(&inserted));
    }

    /// CALM1/2/3 are one protein: the tie-break picks the same accession
    /// whatever order BLAST lists them in
    #[test]
    fn calmodulin_tie_break_is_stable() {
        let rows = |order: [&str; 3]| -> Vec<PolymerBlastHit> {
            order
                .iter()
                .map(|s| blast_row("p0", s, 100.0, 0, (1, 148), 148, 300.0))
                .collect()
        };
        for order in [
            [
                "sp|P0DP25|CALM3_HUMAN",
                "sp|P0DP23|CALM1_HUMAN",
                "sp|P0DP24|CALM2_HUMAN",
            ],
            [
                "sp|P0DP24|CALM2_HUMAN",
                "sp|P0DP25|CALM3_HUMAN",
                "sp|P0DP23|CALM1_HUMAN",
            ],
        ] {
            let best = best_hits(&rows(order), &HashMap::new());
            assert_eq!(best["p0"].accession, "P0DP23");
            assert_eq!(best["p0"].match_method, ALIGNED);
        }
    }

    /// The top bit score wins over a lower accession; on a tie a canonical
    /// entry wins over an isoform; a query with nothing passing gets nothing
    #[test]
    fn best_hit_order() {
        let rows = vec![
            blast_row("p0", "sp|A11111|X", 98.0, 2, (1, 100), 100, 190.0),
            blast_row("p0", "sp|B22222|X", 100.0, 0, (1, 100), 100, 200.0),
            blast_row("p1", "sp|C33333-2|X", 100.0, 0, (1, 100), 100, 200.0),
            blast_row("p1", "sp|D44444|X", 100.0, 0, (1, 100), 100, 200.0),
            blast_row("p2", "tr|E55555|X", 80.0, 20, (1, 100), 100, 150.0),
        ];
        let best = best_hits(&rows, &HashMap::new());
        assert_eq!(best["p0"].accession, "B22222");
        assert_eq!(best["p0"].identity, 100.0);
        assert_eq!(
            (best["p0"].reference_start, best["p0"].reference_end),
            (10, 109)
        );
        assert_eq!(best["p1"].accession, "D44444");
        assert!(!best.contains_key("p2"));
    }

    #[test]
    fn accession_from_subject() {
        assert_eq!(accession("sp|P04585|POL_HV1H2"), "P04585");
        assert_eq!(accession("tr|A0A0B4J2F0|A0A0B4J2F0_HUMAN"), "A0A0B4J2F0");
        assert_eq!(accession("P12345"), "P12345");
    }

    /// The outfmt's file reads back
    #[test]
    fn reads_polymer_blast_tsv() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("blast.polymers.swissprot.tsv");
        fs::write(
            &file,
            "p0\tsp|P04585|POL_HV1H2\t96.970\t99\t3\t0\t1\t99\t489\t587\t1.2e-60\t195\t99\t1435\t96\n",
        )
        .unwrap();
        let rows = read_hits(&file).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].qlen, rows[0].slen, rows[0].mismatch, rows[0].nident),
            (99, 1435, 3, 96)
        );
        let best = best_hits(&rows, &HashMap::new());
        assert_eq!(best["p0"].accession, "P04585");
        assert_eq!(
            (best["p0"].reference_start, best["p0"].reference_end),
            (489, 587)
        );
    }

    /// A chain under 20 residues and a DNA chain get `none` with no index
    /// read; two chains with the same residues share one lookup
    #[test]
    fn short_and_nucleic_chains_get_none() {
        let dir = tempdir().unwrap();
        let mut chains = vec![
            chain(1, "protein", "ACDEFGHIKLMNPQRSTVW"),
            chain(2, "dna", "ACGTACGTACGTACGTACGTACGT"),
            chain(3, "protein", "ACDEFGHIKLMNPQRSTVW"),
        ];
        let warnings =
            lookup_chains(&mut chains, None, &[], dir.path(), dir.path(), 1).unwrap();
        assert!(warnings.is_empty());
        for c in &chains {
            assert_eq!(c.reference, Some(none()));
        }
    }

    /// No PDB index on the host: the chain is left untried, with a warning,
    /// rather than given an answer that would stick
    #[test]
    fn no_index_leaves_chains_untried() {
        let dir = tempdir().unwrap();
        let mut chains =
            vec![chain(1, "protein", PROTEASE), chain(2, "protein", "ACDEF")];
        let warnings =
            lookup_chains(&mut chains, None, &[], dir.path(), dir.path(), 1).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no PDB index"));
        assert_eq!(chains[0].reference, None);
        assert_eq!(chains[1].reference, Some(none()));
    }

    #[test]
    fn declared_accession_that_is_no_reference_warns() {
        let mut a = chain(1, "protein", PROTEASE);
        a.reference = Some(PolymerLookup {
            match_method: PDB.into(),
            hit: Some(PolymerHit {
                uniprot: UniprotEntry {
                    uniprot_id: "P04585".into(),
                    ..Default::default()
                },
                query_start: 1,
                query_end: 99,
                reference_start: 489,
                reference_end: 587,
                identity: 96.0,
            }),
        });
        let declared = vec!["P04585-2".to_string(), "P03367".to_string()];
        let got = declared_not_referenced(&declared, &[a.clone()]);
        assert_eq!(got.len(), 1);
        assert!(got[0].contains(r#""P03367""#));

        a.reference = Some(none());
        assert!(declared_not_referenced(&declared, &[a]).is_empty());
    }

    /// A declared PDB ID warns only when SIFTS maps it, every protein
    /// polymer was looked up, and none is related to it (84136: 4tq8's
    /// transthyretin entry on an epoxide hydrolase)
    #[test]
    fn declared_pdb_that_no_chain_matches_warns() {
        let listed = vec!["P02766".to_string()];
        let got =
            declared_pdb_not_matched("4tq8", &listed, &[Some(false), Some(false)])
                .unwrap();
        assert!(got.contains(r#""4tq8""#), "{got}");
        assert!(got.contains("P02766"), "{got}");

        // One related polymer is enough
        assert_eq!(
            declared_pdb_not_matched("4tq8", &listed, &[Some(false), Some(true)]),
            None
        );
        // A polymer not looked up: not known
        assert_eq!(
            declared_pdb_not_matched("4tq8", &listed, &[Some(false), None]),
            None
        );
        // Not in SIFTS, or no protein polymer: nothing to compare
        assert_eq!(declared_pdb_not_matched("4tq8", &[], &[Some(false)]), None);
        assert_eq!(declared_pdb_not_matched("4tq8", &listed, &[]), None);
    }

    /// Protein polymers long enough to look up, each with whether it is
    /// related, or `None` when not looked up; peptides and DNA left out
    #[test]
    fn related_flags_of_the_polymers() {
        let queries = vec![
            query(PROTEASE, None, &[]),
            query(CALM40, None, &[]),
            query("ACDEFGHIK", None, &[]),
            PolymerQuery {
                polymer_type: "dna".into(),
                ..query("ACGTACGTACGTACGTACGTACGT", None, &[])
            },
            query(&PROTEASE[..50], None, &[]),
        ];
        let lookups =
            vec![Some(none()), None, Some(none()), Some(none()), Some(none())];
        let related = HashSet::from([0, 2, 3]);
        assert_eq!(
            related_flags(&queries, &lookups, &related),
            vec![Some(true), None, Some(false)]
        );
    }

    /// A polymer is related to its declared PDB entry when a SIFTS accession
    /// of it aligns at 70% or more over 20 or more residues, whatever the
    /// coverage; other rows, and rows of accessions SIFTS does not list for
    /// it, do not count
    #[test]
    fn related_to_pdb_rows() {
        let listed: HashSet<(String, String)> = [
            ("p0".to_string(), "P01857".to_string()),
            ("p1".to_string(), "P01857".to_string()),
            ("p2".to_string(), "P01857".to_string()),
            ("p3".to_string(), "P01857".to_string()),
        ]
        .into();
        let rows = [
            // p0: the constant half of a 220-residue Fab chain, 100%
            blast_row("p0", "P01857", 100.0, 0, (111, 210), 220, 200.0),
            // p1: 69% identical
            blast_row("p1", "P01857", 69.0, 31, (1, 100), 220, 80.0),
            // p2: 19 residues
            blast_row("p2", "P01857", 100.0, 0, (1, 19), 220, 40.0),
            // p3: an accession SIFTS does not list for it; p4: no SIFTS
            blast_row("p3", "sp|P04585|POL_HV1H2", 100.0, 0, (1, 99), 99, 200.0),
            blast_row("p4", "P01857", 100.0, 0, (1, 99), 99, 200.0),
        ];
        assert_eq!(related_to_pdb(&rows, &listed), HashSet::from([0]));
    }

    /// Step 2 reports which polymers their declared PDB entry is about, by
    /// blastp: not one whose entry is another protein (84136's case); yes for
    /// a fusion whose entry SIFTS maps only over a third of it (an antibody's
    /// constant half), though that is no match; yes when a declared UniProt
    /// ID outscores the SIFTS accession and becomes the reference (1hxw)
    #[test]
    fn declared_step_reports_polymers_related_to_their_pdb() {
        if !has_blastp() {
            return;
        }
        let dir = tempdir().unwrap();
        let cache = dir.path().join("uniprot");
        // A mutant protease for SIFTS: every tenth residue changed, 90%
        let mutant: String = PROTEASE
            .chars()
            .enumerate()
            .map(|(i, c)| match (i % 10 == 5, c) {
                (true, 'A') => 'S',
                (true, _) => 'A',
                (false, c) => c,
            })
            .collect();
        cache_with(
            &cache,
            &[
                ("P0DP23", CALM40),
                ("Q99999", &mutant),
                ("P04585", PROTEASE),
            ],
        );
        let sifts = sifts_with(
            dir.path(),
            &[("9cal", "A", "P0DP23"), ("9mut", "A", "Q99999")],
        );
        let queries = vec![
            query(PROTEASE, Some("9cal"), &[]),
            query(&format!("{PROTEASE}{CALM40}"), Some("9cal"), &[]),
            query(PROTEASE, Some("9mut"), &["P04585"]),
        ];
        let wanted: Vec<usize> = (0..queries.len()).collect();
        let (got, _, warnings, related) =
            declared_matches(&queries, &wanted, &sifts, dir.path(), Some(&cache))
                .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(related, HashSet::from([1, 2]));
        assert_eq!(got[&0], Declared::NoMatch);
        assert_eq!(got[&1], Declared::NoMatch);
        let declared = matched(&got, 2);
        assert_eq!(
            (declared.accession.as_str(), declared.match_method),
            ("P04585", DECLARED)
        );
    }
}
