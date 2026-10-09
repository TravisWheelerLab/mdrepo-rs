//! A polymer's UniProt reference. Per distinct polymer in a simulation:
//!
//! 1. Not protein, or under `MIN_RESIDUES` residues: `none`. BLAST cannot
//!    reach its e-value with a short query, and a short one often matches
//!    many entries identically.
//! 2. **The declared PDB entry** (`pdb`; Ken, 2026-10-09). When the
//!    simulation declares a PDB ID, its protein chains (`pdb_seqres.txt`)
//!    are compared with ours: every residue of ours must equal one of theirs,
//!    in order, with theirs allowed extra residues at either end and, inside,
//!    only where our backbone is broken (a missing loop; `ImportChain::
//!    breaks`). No mismatches. SIFTS (`pdb_chain_uniprot.tsv`) then gives the
//!    UniProt entry for the matching chain, the segment covering most of our
//!    residues (a fusion's largest).
//! 3. **BLAST** (`aligned`): Swiss-Prot, then TrEMBL for what Swiss-Prot did
//!    not pass. A hit passes with identity >= 95%, at most 3 mismatches, and
//!    the alignment covering >= 90% of the chain. The best passing hit has
//!    the top bit score; ties go to a canonical entry over an isoform, then
//!    the lowest accession.
//! 4. Otherwise `none`.
//!
//! The reference is md_polymer's, shared by every simulation with the same
//! residues, and set once: the import writes it only to a polymer never
//! looked up (`ops::set_polymer_reference`). Step 2 reads the simulation's
//! own PDB ID, so simulations sharing a polymer can disagree; the first to
//! import decides, and a later one that disagrees gets a warning
//! (`import::reference_disagreements`). Ken relaxed "no simulation may point
//! to the wrong UniProt entry" to allow this (2026-10-09), in place of
//! matching against the whole PDB.
//!
//! Nothing here writes to the database. A polymer whose lookup could not be
//! finished (no PDB index on the host, a UniProt fetch that failed) gets no
//! result at all and is left for a later run, rather than a `none` that
//! would stick.

use crate::{
    process::{blast_db, get_uniprot_entry, run_blastp, uniprot_entry_from_json},
    types::{ImportChain, PolymerHit, PolymerLookup, UniprotDb, UniprotEntry},
};
use anyhow::{Result, anyhow};
use libmdrepo::common::file_exists;
use log::{debug, info, warn};
use rayon::prelude::*;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
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

/// Enough targets that every hit tied at the top bit score is listed, so the
/// tie-break sees them all: calmodulin's sequence ties more than 20 Swiss-Prot
/// entries, and at the old 20 CALM1 (P0DP23) was cut from the list
const MAX_TARGET_SEQS: &str = "500";

/// Plain `-outfmt 6` plus the lengths the coverage floor needs
const BLAST_OUTFMT: &str = "6 qaccver saccver pident length mismatch gapopen \
                            qstart qend sstart send evalue bitscore qlen slen";

/// Every PDB chain's sequence (wwPDB's `derived_data/pdb_seqres.txt`) and
/// SIFTS's chain-to-UniProt segments (`pdb_chain_uniprot.tsv`), both
/// uncompressed, under the BLAST dir and refreshed with it (`just pdb-index`)
pub const PDB_SEQRES: &str = "pdb/pdb_seqres.txt";
pub const PDB_SIFTS: &str = "pdb/pdb_chain_uniprot.tsv";

pub const PDB: &str = "pdb";
pub const ALIGNED: &str = "aligned";
pub const NONE: &str = "none";

// --------------------------------------------------
/// A reference before its UniProt entry is fetched
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub match_method: &'static str,
    pub accession: String,
    pub query_start: i32,
    pub query_end: i32,
    pub reference_start: i32,
    pub reference_end: i32,

    /// BLAST's percent identity. None for a PDB match: it is measured
    /// against the UniProt sequence once that is fetched, over `pairs`.
    pub identity: Option<f64>,

    /// A PDB match's residue pairs, 1-based: each of our residues in the
    /// segment and its UniProt position. Empty for BLAST.
    pub pairs: Vec<(i32, i32)>,
}

// --------------------------------------------------
/// One SIFTS row: residues `res_beg..=res_end` of a PDB chain (SEQRES
/// numbering, so positions in its `pdb_seqres.txt` sequence) are
/// `sp_beg..=sp_end` of a UniProt entry
#[derive(Debug, Clone, PartialEq)]
pub struct SiftsSegment {
    pub pdb: String,
    pub chain: String,
    pub accession: String,
    pub res_beg: i32,
    pub res_end: i32,
    pub sp_beg: i32,
    pub sp_end: i32,
}

/// SIFTS's segments by (PDB ID, chain ID)
pub type Segments = HashMap<(String, String), Vec<SiftsSegment>>;

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
}

// --------------------------------------------------
/// One polymer to look up: what it is, and the declared PDB ID of the
/// simulation that decides it
#[derive(Debug, Clone, PartialEq)]
pub struct PolymerQuery {
    pub polymer_type: String,
    pub num_residues: usize,
    pub sequence: String,
    pub breaks: Vec<usize>,
    pub pdb_id: Option<String>,
}

// --------------------------------------------------
/// Look up every distinct polymer among one simulation's `chains` and set
/// each chain's `reference`. `pdb_id` is the simulation's declared PDB ID.
/// Returns warnings for the submitter.
pub fn lookup_chains(
    chains: &mut [ImportChain],
    pdb_id: Option<&str>,
    blast_dir: &Path,
    processed_dir: &Path,
    num_threads: usize,
) -> Result<Vec<String>> {
    // Distinct polymers, in chain order: (type, residues) is md_polymer's
    // key. The first chain of each gives the sequence and the breaks.
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
                breaks: chain.breaks.clone(),
                pdb_id: pdb_id.map(str::to_string),
            });
        }
    }

    let (lookups, warnings) =
        lookup_polymers(&queries, blast_dir, processed_dir, num_threads, None)?;

    for chain in chains.iter_mut() {
        let i = keys
            .iter()
            .position(|k| (&k.0, &k.1) == (&chain.polymer_type, &chain.residues))
            .ok_or_else(|| anyhow!("chain {} has no polymer", chain.chain_order))?;
        chain.reference = lookups[i].clone();
    }

    Ok(warnings)
}

// --------------------------------------------------
/// Steps 1-4 for many polymers at once, each against its own declared PDB
/// ID: the PDB index is read once, BLAST runs once over every polymer that
/// needs it (its files go in `work_dir`), and each UniProt entry is fetched
/// once (or read from `uniprot_cache`, see `fetch_entries`). Returns a
/// lookup per query, None where it could not be finished
/// (left untried), and warnings.
pub fn lookup_polymers(
    queries: &[PolymerQuery],
    blast_dir: &Path,
    work_dir: &Path,
    num_threads: usize,
    uniprot_cache: Option<&Path>,
) -> Result<(Vec<Option<PolymerLookup>>, Vec<String>)> {
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
        return Ok((lookups, warnings));
    }

    let seqres = blast_dir.join(PDB_SEQRES);
    let sifts = blast_dir.join(PDB_SIFTS);
    if !(seqres.is_file() && sifts.is_file()) {
        warnings.push(format!(
            "The UniProt references of the chains were not looked up: no PDB \
             index ({} and {})",
            seqres.display(),
            sifts.display()
        ));
        return Ok((lookups, warnings));
    }

    // 2. The declared PDB entries, read in one pass each
    let pdb_ids: HashSet<String> = wanted
        .iter()
        .filter_map(|&i| queries[i].pdb_id.as_ref().map(|p| p.to_lowercase()))
        .collect();
    let entries = entries_chains(&seqres, &pdb_ids)?;
    let pairs: HashSet<(String, String)> = entries
        .iter()
        .flat_map(|(pdb, chains)| chains.iter().map(|(c, _)| (pdb.clone(), c.clone())))
        .collect();
    let segments = sifts_segments(&sifts, &pairs)?;

    let mut candidates: HashMap<usize, Candidate> = HashMap::new();
    for &i in &wanted {
        let q = &queries[i];
        if let Some(pdb) = q.pdb_id.as_ref().map(|p| p.to_lowercase())
            && let Some(chains) = entries.get(&pdb)
            && let Some(c) =
                pdb_candidate(&q.sequence, &q.breaks, &pdb, chains, &segments)
        {
            candidates.insert(i, c);
        }
    }
    let uniprot = fetch_entries(
        candidates.values().map(|c| c.accession.as_str()),
        uniprot_cache,
    );

    let mut to_blast = vec![];
    for &i in &wanted {
        let Some(candidate) = candidates.get(&i) else {
            to_blast.push(i);
            continue;
        };
        match &uniprot[&candidate.accession] {
            Ok(entry) => match make_hit(candidate, &queries[i].sequence, entry) {
                Some(hit) => lookups[i] = Some(lookup(candidate, hit)),
                None => to_blast.push(i),
            },
            Err(e) => warnings.push(not_recorded(candidate, e)),
        }
    }

    // 3. BLAST, once for all
    if !to_blast.is_empty() {
        let blast_queries: Vec<(String, &str)> = to_blast
            .iter()
            .map(|&i| (format!("p{i}"), queries[i].sequence.as_str()))
            .collect();
        let found = blast_candidates(&blast_queries, blast_dir, work_dir, num_threads)?;
        let uniprot =
            fetch_entries(found.values().map(|c| c.accession.as_str()), uniprot_cache);
        for &i in &to_blast {
            let Some(candidate) = found.get(&format!("p{i}")) else {
                // 4. Nothing passed
                lookups[i] = Some(none());
                continue;
            };
            match &uniprot[&candidate.accession] {
                Ok(entry) => match make_hit(candidate, &queries[i].sequence, entry) {
                    Some(hit) => lookups[i] = Some(lookup(candidate, hit)),
                    None => {
                        debug!(
                            "{}'s range {}-{} is past its sequence; no reference",
                            candidate.accession,
                            candidate.reference_start,
                            candidate.reference_end
                        );
                        lookups[i] = Some(none());
                    }
                },
                Err(e) => warnings.push(not_recorded(candidate, e)),
            }
        }
    }

    Ok((lookups, warnings))
}

/// Each accession's UniProt entry, fetched once, several at a time, with
/// two retries; or why it could not be had. With `cache`, an entry already
/// there (`<accession>.json`: when it was fetched, and UniProt's JSON) is
/// read instead of fetched, and a fetched one is saved, so a later run
/// sees the same entries: a backfill's real run writes what its dry run
/// reported.
fn fetch_entries<'a>(
    accessions: impl Iterator<Item = &'a str>,
    cache: Option<&Path>,
) -> HashMap<String, std::result::Result<UniprotEntry, String>> {
    let mut accessions: Vec<&str> = accessions.collect();
    accessions.sort_unstable();
    accessions.dedup();
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
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(UNIPROT_FETCHES)
        .build();
    let results: Vec<(String, std::result::Result<UniprotEntry, String>)> = match pool {
        Ok(pool) => pool.install(|| {
            accessions
                .par_iter()
                .map(|acc| (acc.to_string(), fetch(acc)))
                .collect()
        }),
        Err(_) => accessions
            .iter()
            .map(|acc| (acc.to_string(), fetch(acc)))
            .collect(),
    };
    info!(
        "{} UniProt entries{}",
        results.len(),
        cache.map_or(String::new(), |d| format!(" (cache {})", d.display()))
    );
    results.into_iter().collect()
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
    fs::create_dir_all(dir)?;
    let file = cache_file(dir, &entry.uniprot_id);
    let tmp = file.with_extension(format!("json.{}", std::process::id()));
    let cached = Cached {
        fetched_at,
        response: response.clone(),
    };
    fs::write(&tmp, serde_json::to_vec(&cached)?)?;
    fs::rename(&tmp, &file)?;
    Ok(())
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
/// The hit for a candidate whose UniProt entry is in hand. A PDB match's
/// identity is the share of its residue pairs where ours equals UniProt's.
/// None when a range does not fit the sequences: SIFTS read against an
/// older UniProt release than the one served now.
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
    let identity = match candidate.identity {
        Some(identity) => identity,
        None => {
            let (ours, theirs) = (sequence.as_bytes(), uniprot.sequence.as_bytes());
            if candidate.pairs.is_empty() {
                return None;
            }
            let same = candidate
                .pairs
                .iter()
                .filter(|&&(q, r)| ours[q as usize - 1] == theirs[r as usize - 1])
                .count();
            100.0 * same as f64 / candidate.pairs.len() as f64
        }
    };
    Some(PolymerHit {
        uniprot: uniprot.clone(),
        query_start: qs,
        query_end: qe,
        reference_start: rs,
        reference_end: re,
        identity,
    })
}

// --------------------------------------------------
/// The protein chains of one PDB entry in `pdb_seqres.txt`, as (chain ID,
/// sequence), in chain ID order
pub fn entry_chains(seqres: &Path, pdb_id: &str) -> Result<Vec<(String, String)>> {
    let pdb = pdb_id.to_lowercase();
    let ids: HashSet<String> = [pdb.clone()].into_iter().collect();
    Ok(entries_chains(seqres, &ids)?
        .remove(&pdb)
        .unwrap_or_default())
}

/// The protein chains of each of `pdb_ids` (lower case) in
/// `pdb_seqres.txt`, read in one pass: by PDB ID, (chain ID, sequence) in
/// chain ID order. An ID with no protein chain is absent.
pub fn entries_chains(
    seqres: &Path,
    pdb_ids: &HashSet<String>,
) -> Result<HashMap<String, Vec<(String, String)>>> {
    let mut found: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if pdb_ids.is_empty() {
        return Ok(found);
    }
    let file = File::open(seqres).map_err(|e| anyhow!("{}: {e}", seqres.display()))?;

    // `>101m_A mol:protein length:154  MYOGLOBIN`, then the sequence
    let mut current: Option<(String, usize)> = None;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| anyhow!("{}: {e}", seqres.display()))?;
        if let Some(header) = line.strip_prefix('>') {
            current = None;
            let mut fields = header.split_whitespace();
            let id = fields.next().unwrap_or("");
            if fields.next() != Some("mol:protein") {
                continue;
            }
            // Keep the chain ID as written: SIFTS's is case-sensitive
            if let Some((pdb, chain)) = id.split_once('_') {
                let pdb = pdb.to_lowercase();
                if pdb_ids.contains(&pdb) {
                    let chains = found.entry(pdb.clone()).or_default();
                    chains.push((chain.to_string(), String::new()));
                    current = Some((pdb, chains.len() - 1));
                }
            }
        } else if let Some((pdb, i)) = &current
            && let Some(chains) = found.get_mut(pdb)
        {
            chains[*i].1.push_str(line.trim());
        }
    }
    for chains in found.values_mut() {
        chains.sort();
    }
    Ok(found)
}

// --------------------------------------------------
/// Where each of our residues sits in `theirs` (0-based), when ours is
/// theirs with residues missing at either end and, inside, only at our
/// `breaks`. Each unbroken piece of ours must appear whole, in order; a piece
/// is placed at its first occurrence after the one before it. None when
/// they do not match.
pub fn place(ours: &str, breaks: &[usize], theirs: &str) -> Option<Vec<usize>> {
    if ours.is_empty() {
        return None;
    }
    let mut cuts: Vec<usize> = breaks
        .iter()
        .copied()
        .filter(|&b| b > 0 && b < ours.len())
        .collect();
    cuts.sort_unstable();
    cuts.dedup();

    let mut positions = Vec::with_capacity(ours.len());
    let mut from = 0;
    let mut start = 0;
    for end in cuts.into_iter().chain([ours.len()]) {
        let piece = &ours[start..end];
        let at = from + theirs.get(from..)?.find(piece)?;
        positions.extend(at..at + piece.len());
        from = at + piece.len();
        start = end;
    }
    Some(positions)
}

// --------------------------------------------------
/// SIFTS's segments for the given (PDB ID, chain ID)s
pub fn sifts_segments(
    sifts: &Path,
    pairs: &HashSet<(String, String)>,
) -> Result<Segments> {
    let mut found: Segments = HashMap::new();
    if pairs.is_empty() {
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
        if fields.len() < 9 {
            continue;
        }
        let key = (fields[0].to_lowercase(), fields[1].to_string());
        if !pairs.contains(&key) {
            continue;
        }
        let num = |i: usize| fields[i].trim().parse::<i32>().ok();
        let (Some(res_beg), Some(res_end), Some(sp_beg), Some(sp_end)) =
            (num(3), num(4), num(7), num(8))
        else {
            continue;
        };
        found.entry(key.clone()).or_default().push(SiftsSegment {
            pdb: key.0,
            chain: key.1,
            accession: fields[2].to_string(),
            res_beg,
            res_end,
            sp_beg,
            sp_end,
        });
    }
    Ok(found)
}

// --------------------------------------------------
/// Step 2's answer for one polymer, given its sequence and breaks and the
/// declared entry's chains (chain ID, sequence) with their SIFTS segments.
/// Of every chain that matches and every segment of it, the one covering
/// most of our residues is taken; ties go to the lowest accession, then the
/// lowest chain ID.
pub fn pdb_candidate(
    sequence: &str,
    breaks: &[usize],
    pdb: &str,
    chains: &[(String, String)],
    segments: &Segments,
) -> Option<Candidate> {
    // (residues covered, accession, chain), pairs
    type Best<'a> = ((usize, &'a str, &'a str), Vec<(i32, i32)>);
    let mut best: Option<Best> = None;
    for (chain, theirs) in chains {
        let Some(positions) = place(sequence, breaks, theirs) else {
            continue;
        };
        let key = (pdb.to_string(), chain.clone());
        for seg in segments.get(&key).into_iter().flatten() {
            if seg.res_beg < 1 || seg.res_end < seg.res_beg || seg.sp_beg < 1 {
                continue;
            }
            // Our residues inside the segment, paired with UniProt positions
            let pairs: Vec<(i32, i32)> = positions
                .iter()
                .enumerate()
                .filter_map(|(q, &t)| {
                    let t = t as i32 + 1;
                    (seg.res_beg..=seg.res_end)
                        .contains(&t)
                        .then(|| (q as i32 + 1, seg.sp_beg + t - seg.res_beg))
                })
                .collect();
            if pairs.is_empty() {
                continue;
            }
            let better = match &best {
                None => true,
                Some(((n, acc, ch), _)) => {
                    (
                        pairs.len(),
                        std::cmp::Reverse(seg.accession.as_str()),
                        std::cmp::Reverse(chain.as_str()),
                    ) > (*n, std::cmp::Reverse(*acc), std::cmp::Reverse(*ch))
                }
            };
            if better {
                best = Some((
                    (pairs.len(), seg.accession.as_str(), chain.as_str()),
                    pairs,
                ));
            }
        }
    }

    let ((_, accession, chain), pairs) = best?;
    let (first, last) = (pairs[0], pairs[pairs.len() - 1]);
    debug!(
        "PDB {pdb} chain {chain} has the residues: {accession} {}-{}",
        first.1, last.1
    );
    Some(Candidate {
        match_method: PDB,
        accession: accession.to_string(),
        query_start: first.0,
        query_end: last.0,
        reference_start: first.1,
        reference_end: last.1,
        identity: None,
        pairs,
    })
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
        for (id, candidate) in best_hits(&read_hits(&results)?) {
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

/// Whether a BLAST hit is close enough to be the chain's reference
pub fn passes(hit: &PolymerBlastHit) -> bool {
    let covered = (hit.qend.max(hit.qstart) - hit.qstart.min(hit.qend) + 1) as f64;
    hit.qlen > 0
        && hit.pident >= MIN_IDENTITY
        && hit.mismatch <= MAX_MISMATCHES
        && covered / hit.qlen as f64 >= MIN_COVERAGE
}

/// The accession in `sp|P04585|POL_HV1H2` or `tr|A0A...|...`
pub fn accession(saccver: &str) -> String {
    let re = Regex::new(r"^(?:sp|tr)[|]([^|]+)[|]").expect("valid regex");
    re.captures(saccver)
        .and_then(|c| c.get(1))
        .map_or(saccver, |m| m.as_str())
        .to_string()
}

/// The best passing hit for each query: the top bit score, then a canonical
/// entry over an isoform (`P12345-2`), then the lowest accession
pub fn best_hits(hits: &[PolymerBlastHit]) -> HashMap<String, Candidate> {
    let mut by_query: HashMap<&str, Vec<&PolymerBlastHit>> = HashMap::new();
    for hit in hits.iter().filter(|h| passes(h)) {
        by_query.entry(hit.qaccver.as_str()).or_default().push(hit);
    }

    let mut best = HashMap::new();
    for (query, mut hits) in by_query {
        hits.sort_by(|a, b| {
            let (acc_a, acc_b) = (accession(&a.saccver), accession(&b.saccver));
            b.bitscore
                .total_cmp(&a.bitscore)
                .then_with(|| acc_a.contains('-').cmp(&acc_b.contains('-')))
                .then_with(|| acc_a.cmp(&acc_b))
        });
        let top = hits[0];
        let tied: Vec<String> = hits
            .iter()
            .filter(|h| h.bitscore == top.bitscore)
            .map(|h| accession(&h.saccver))
            .collect();
        if tied.len() > 1 {
            debug!(
                "{query}: bit score {} ties {}; took {}",
                top.bitscore,
                tied.join(", "),
                tied[0]
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
                identity: Some(top.pident),
                pairs: vec![],
            },
        );
    }
    best
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

    fn seg(
        pdb: &str,
        chain: &str,
        acc: &str,
        res: (i32, i32),
        sp: (i32, i32),
    ) -> SiftsSegment {
        SiftsSegment {
            pdb: pdb.into(),
            chain: chain.into(),
            accession: acc.into(),
            res_beg: res.0,
            res_end: res.1,
            sp_beg: sp.0,
            sp_end: sp.1,
        }
    }

    fn segments(rows: Vec<SiftsSegment>) -> Segments {
        let mut map: Segments = HashMap::new();
        for s in rows {
            map.entry((s.pdb.clone(), s.chain.clone()))
                .or_default()
                .push(s);
        }
        map
    }

    fn pair(pdb: &str, chain: &str) -> (String, String) {
        (pdb.into(), chain.into())
    }

    fn blast_row(
        q: &str,
        s: &str,
        pident: f64,
        mismatch: u32,
        qse: (u32, u32),
        qlen: u32,
        bits: f64,
    ) -> PolymerBlastHit {
        PolymerBlastHit {
            qaccver: q.into(),
            saccver: s.into(),
            pident,
            length: qse.1 - qse.0 + 1,
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

    /// One entry's protein chains come out of `pdb_seqres.txt`, and SIFTS's
    /// rows are read as written
    #[test]
    fn reads_entry_chains_and_sifts() {
        let dir = tempdir().unwrap();
        let seqres = dir.path().join("pdb_seqres.txt");
        fs::write(
            &seqres,
            format!(
                ">1a2z_A mol:protein length:3  OTHER\nAAA\n\
                 >1a30_A mol:protein length:99  HIV-1 PROTEASE\n{PROTEASE}\n\
                 >1a30_B mol:protein length:99  HIV-1 PROTEASE\n{PROTEASE}\n\
                 >1a30_C mol:protein length:3  TRIPEPTIDE GLU-ASP-LEU\nEDL\n\
                 >1a30_D mol:na length:4  DNA\nACGT\n\
                 >1a31_A mol:protein length:3  NEXT\nGGG\n"
            ),
        )
        .unwrap();
        let got = entry_chains(&seqres, "1A30").unwrap();
        assert_eq!(
            got,
            vec![
                ("A".to_string(), PROTEASE.to_string()),
                ("B".to_string(), PROTEASE.to_string()),
                ("C".to_string(), "EDL".to_string()),
            ]
        );
        assert!(entry_chains(&seqres, "9xyz").unwrap().is_empty());

        let sifts = dir.path().join("pdb_chain_uniprot.tsv");
        fs::write(
            &sifts,
            "# 2026/10/04 - 08:26 | PDB: 40.26 | UniProt: 2026.03\n\
             PDB\tCHAIN\tSP_PRIMARY\tRES_BEG\tRES_END\tPDB_BEG\tPDB_END\tSP_BEG\tSP_END\n\
             101m\tA\tP02185\t1\t154\t0\t153\t1\t154\n\
             1a30\tA\tP04585\t1\t99\t1\t99\t489\t587\n\
             3uon\tA\tP08172\t378\t467\t377\t\t377\t466\n",
        )
        .unwrap();
        let pairs: HashSet<_> =
            [pair("1a30", "A"), pair("3uon", "A")].into_iter().collect();
        let got = sifts_segments(&sifts, &pairs).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[&pair("1a30", "A")],
            vec![seg("1a30", "A", "P04585", (1, 99), (489, 587))]
        );
        // A blank PDB_END does not matter: only SEQRES and UniProt positions
        // are read
        assert_eq!(got[&pair("3uon", "A")][0].sp_end, 466);
    }

    /// Ours may lack residues at either end of theirs, and inside only at a
    /// break; never a mismatch or a residue theirs lacks
    #[test]
    fn placing_a_chain() {
        let theirs = "MGSHHHHHHACDEFGHIKLMNPQRSTVW";
        // Ends trimmed
        assert_eq!(place("ACDEF", &[], theirs), Some((9..14).collect()));
        // A missing loop (GHIK) at a break
        assert_eq!(
            place("ACDEFLMNP", &[5], theirs),
            Some(vec![9, 10, 11, 12, 13, 18, 19, 20, 21])
        );
        // The same gap with no break there
        assert_eq!(place("ACDEFLMNP", &[], theirs), None);
        // A mismatch, and a residue theirs lacks
        assert_eq!(place("ACDEY", &[], theirs), None);
        assert_eq!(place("ACDEWF", &[4], theirs), None);
        // Pieces must stay in order
        assert_eq!(place("LMNACD", &[3], theirs), None);
        // A break at either end changes nothing
        assert_eq!(place("ACDEF", &[0, 5], theirs), Some((9..14).collect()));
    }

    fn protease_entry() -> (Vec<(String, String)>, Segments) {
        let chains = vec![
            ("A".to_string(), PROTEASE.to_string()),
            ("B".to_string(), PROTEASE.to_string()),
            ("C".to_string(), "EDL".to_string()),
        ];
        let segs = segments(vec![
            seg("1a30", "A", "P04585", (1, 99), (489, 587)),
            seg("1a30", "B", "P04585", (1, 99), (489, 587)),
        ]);
        (chains, segs)
    }

    /// 1a30's protease (99 residues), declared as 1a30, takes P04585 489-587
    /// by `pdb`
    #[test]
    fn protease_by_declared_pdb() {
        let (chains, segs) = protease_entry();
        let got = pdb_candidate(PROTEASE, &[], "1a30", &chains, &segs).unwrap();
        assert_eq!(got.match_method, PDB);
        assert_eq!(got.accession, "P04585");
        assert_eq!((got.query_start, got.query_end), (1, 99));
        assert_eq!((got.reference_start, got.reference_end), (489, 587));
        assert_eq!(got.pairs.len(), 99);
        assert_eq!(got.identity, None);
    }

    /// A chain trimmed at both ends, and one with a missing loop at a break,
    /// map their positions through the match
    #[test]
    fn trimmed_and_looped_chains_map_positions() {
        let (chains, segs) = protease_entry();
        let trimmed = &PROTEASE[5..95];
        let got = pdb_candidate(trimmed, &[], "1a30", &chains, &segs).unwrap();
        assert_eq!((got.query_start, got.query_end), (1, 90));
        assert_eq!((got.reference_start, got.reference_end), (494, 583));

        let looped = format!("{}{}", &PROTEASE[..40], &PROTEASE[50..]);
        let got = pdb_candidate(&looped, &[40], "1a30", &chains, &segs).unwrap();
        assert_eq!((got.query_start, got.query_end), (1, 89));
        assert_eq!((got.reference_start, got.reference_end), (489, 587));
        // Our 41st residue is the entry's 51st, UniProt 539
        assert_eq!(got.pairs[40], (41, 539));

        // Without the break the loop is not allowed
        assert_eq!(pdb_candidate(&looped, &[], "1a30", &chains, &segs), None);
    }

    /// A mutant does not match its entry (it goes to BLAST), nor does a
    /// chain when the simulation declared no entry with it
    #[test]
    fn a_mutant_does_not_match() {
        let (chains, segs) = protease_entry();
        let mutant = PROTEASE.replacen("GQLKEALLD", "GQWKEALLD", 1);
        assert_eq!(pdb_candidate(&mutant, &[], "1a30", &chains, &segs), None);
        assert_eq!(pdb_candidate(PROTEASE, &[], "1a30", &[], &segs), None);
    }

    /// A fusion (3uon-like: receptor, T4L, receptor) takes the segment
    /// covering most of our residues
    #[test]
    fn fusion_takes_the_largest_segment() {
        let theirs: String = "ACDEFGHIKL".repeat(5);
        let chains = vec![("A".to_string(), theirs.clone())];
        let segs = segments(vec![
            seg("3uon", "A", "P08172", (1, 15), (1, 15)),
            seg("3uon", "A", "P00720", (16, 40), (2, 26)),
            seg("3uon", "A", "P08172", (41, 50), (377, 386)),
        ]);
        let got = pdb_candidate(&theirs, &[], "3uon", &chains, &segs).unwrap();
        assert_eq!(got.accession, "P00720");
        assert_eq!((got.query_start, got.query_end), (16, 40));
        assert_eq!((got.reference_start, got.reference_end), (2, 26));
    }

    /// Two matching chains of one entry that SIFTS maps differently: the
    /// one covering more of ours, then the lowest accession
    #[test]
    fn matching_chains_that_differ() {
        let chains = vec![
            ("A".to_string(), PROTEASE.to_string()),
            ("B".to_string(), PROTEASE.to_string()),
        ];
        let segs = segments(vec![
            seg("9zzz", "A", "Q22222", (1, 99), (1, 99)),
            seg("9zzz", "B", "Q11111", (1, 99), (1, 99)),
        ]);
        let got = pdb_candidate(PROTEASE, &[], "9zzz", &chains, &segs).unwrap();
        assert_eq!(got.accession, "Q11111");

        let segs = segments(vec![
            seg("9zzz", "A", "Q22222", (1, 99), (1, 99)),
            seg("9zzz", "B", "Q11111", (1, 60), (1, 60)),
        ]);
        let got = pdb_candidate(PROTEASE, &[], "9zzz", &chains, &segs).unwrap();
        assert_eq!(got.accession, "Q22222");
    }

    /// A PDB match's identity is measured against UniProt over its pairs,
    /// and a range past the UniProt sequence gives no hit
    #[test]
    fn pdb_hit_identity_and_range() {
        let candidate = Candidate {
            match_method: PDB,
            accession: "Q00000".into(),
            query_start: 1,
            query_end: 4,
            reference_start: 3,
            reference_end: 6,
            identity: None,
            pairs: vec![(1, 3), (2, 4), (3, 5), (4, 6)],
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

    /// A mutant (one mismatch) passes; 1ktt's two chains read as one fail
    /// the 90% coverage floor; 94% identity fails; 4 mismatches fail
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
            let best = best_hits(&rows(order));
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
        let best = best_hits(&rows);
        assert_eq!(best["p0"].accession, "B22222");
        assert_eq!(best["p0"].identity, Some(100.0));
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

    /// The new outfmt's file reads back
    #[test]
    fn reads_polymer_blast_tsv() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("blast.polymers.swissprot.tsv");
        fs::write(
            &file,
            "p0\tsp|P04585|POL_HV1H2\t96.970\t99\t3\t0\t1\t99\t489\t587\t1.2e-60\t195\t99\t1435\n",
        )
        .unwrap();
        let rows = read_hits(&file).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].qlen, rows[0].slen, rows[0].mismatch),
            (99, 1435, 3)
        );
        let best = best_hits(&rows);
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
            lookup_chains(&mut chains, None, dir.path(), dir.path(), 1).unwrap();
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
            lookup_chains(&mut chains, None, dir.path(), dir.path(), 1).unwrap();
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
}
