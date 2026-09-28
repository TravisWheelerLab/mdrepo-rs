//! Protein sequence per chain, read from a processed `full.pdb`
//!
//! This replaces `get_sequence_from_pdb.py`, which grouped residues by chain
//! letter with Biopython. That dropped or joined chains whenever the letters
//! were blank or repeated, which is most MD output:
//!
//!   - dropped: MDR00087701 has five chains with blank IDs, each numbered
//!     from 1. Biopython keys a residue on (chain, number), so chains 2-5
//!     "redefined" chain 1's residues and only 277 residues were stored.
//!   - joined: MDR00097110 has two chains both lettered "A", separated only
//!     by TER, and they were stored as one record.
//!
//! Here the atoms are read in file order and a new chain starts when
//!
//!   1. the chain letter changes;
//!   2. the residue number goes backwards or repeats; or
//!   3. the numbering runs on without a jump, but the backbone does not
//!      connect: C(i) to N(i+1) over 2.0 A, or, where a residue has no N or C
//!      (coarse-grained models are CA only), CA(i) to CA(i+1) over 4.2 A.
//!
//! Rule 3 only applies when the numbers run on, so a chain with a missing loop
//! (which shows as a jump in the numbering) stays one record. pdbrust drops
//! TER records, so TER cannot be used; rule 2 or 3 catches what it marked.
//!
//! **The missing-loop rule is under review and may change (2026-09-28).** The
//! alternatives are splitting at every backbone break, or also splitting a
//! numbering jump whose break is longer than the missing residues could span
//! (about 3.8 A per residue). MDR00077207 is the test case: GLY200 -> ASP202
//! breaks by 4.5 A and GLY313 -> VAL315 by 4.3 A; it is one record today and
//! would be three under split-at-every-break. Two chains sharing a letter
//! where the second continues the numbering with a jump are still joined, as
//! the Python joined them.
//!
//! What is kept from the Python on purpose, so existing sequences only change
//! where the chains were wrong:
//!
//!   - ATOM records only. Biopython treats HETATM residues as hetero and the
//!     script skipped them, so an MSE written as HETATM is not in the sequence.
//!   - The same residue-name table (Biopython 1.87's extended 3-to-1, plus
//!     the protonation and modification names), and the name is columns
//!     18-20, as both Biopython and pdbrust read it.
//!   - A record that is all `X` is left out.
//!
//! What changes on purpose:
//!
//!   - Records are headed by chain ID, not `>1`, `>2`, ... (2026-09-28).
//!     A blank ID is written `_`, and an ID seen before gets a count so every
//!     header is unique: `>A`, `>A_2`, and `>_`, `>_2` for blank ones.
//!   - Only the first MODEL is read. The Python looped over every model into
//!     the same chain, so a multi-model file had its sequence repeated once
//!     per model.

use anyhow::{Result, anyhow, bail};
use pdbrust::Atom;
use std::{collections::HashMap, path::Path};

/// C(i) to N(i+1) in a peptide bond is ~1.33 A.
const MAX_PEPTIDE_BOND: f64 = 2.0;

/// Consecutive CA atoms sit ~3.8 A apart (trans) and ~2.9 A (cis).
const MAX_CA_CA: f64 = 4.2;

/// One residue as it appears in the file, with the atoms the break test needs
#[derive(Debug)]
struct Residue {
    chain_id: String,
    number: i32,
    ins_code: Option<char>,
    name: String,
    n: Option<[f64; 3]>,
    ca: Option<[f64; 3]>,
    c: Option<[f64; 3]>,
}

impl Residue {
    fn new(atom: &Atom) -> Self {
        Residue {
            chain_id: atom.chain_id.clone(),
            number: atom.residue_seq,
            ins_code: atom.ins_code,
            name: atom.residue_name.to_uppercase(),
            n: None,
            ca: None,
            c: None,
        }
    }

    fn is_same(&self, atom: &Atom) -> bool {
        self.chain_id == atom.chain_id
            && self.number == atom.residue_seq
            && self.ins_code == atom.ins_code
            && self.name.eq_ignore_ascii_case(&atom.residue_name)
    }

    fn add(&mut self, atom: &Atom) {
        let xyz = Some([atom.x, atom.y, atom.z]);
        let slot = match atom.name.as_str() {
            "N" => &mut self.n,
            "CA" => &mut self.ca,
            "C" => &mut self.c,
            _ => return,
        };
        // First one wins, which is alt-loc A when there are several.
        if slot.is_none() {
            *slot = xyz;
        }
    }
}

/// Read `pdb` and return its sequences as FASTA text
pub fn fasta_from_pdb(pdb: &Path) -> Result<String> {
    let structure = pdbrust::parse_pdb_file(pdb)
        .map_err(|err| anyhow!("{}: {err}", pdb.display()))?;

    let atoms = match structure.models.first() {
        Some(model) => &model.atoms,
        None => &structure.atoms,
    };

    let fasta = fasta_from_atoms(atoms);
    if fasta.is_empty() {
        bail!(
            r#"Failed to find sequence in structure "{}""#,
            pdb.display()
        );
    }

    Ok(fasta)
}

/// FASTA text for the chains in `atoms`, empty if there is no protein
pub fn fasta_from_atoms(atoms: &[Atom]) -> String {
    let mut fasta = String::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for chain in split_chains(&residues(atoms)) {
        let seq: String = chain.iter().filter_map(|r| one_letter(&r.name)).collect();
        if seq.chars().any(|c| c != 'X') {
            let id = header(&chain[0].chain_id, &mut seen);
            fasta.push_str(&format!(">{id}\n{seq}\n"));
        }
    }

    fasta
}

/// The chain ID as a unique FASTA header: `_` for blank, `A_2` for a repeat
fn header(chain_id: &str, seen: &mut HashMap<String, usize>) -> String {
    let base = match chain_id.trim() {
        "" => "_".to_string(),
        id => id.to_string(),
    };
    let count = seen.entry(base.clone()).or_insert(0);
    *count += 1;
    match (*count, base.as_str()) {
        (1, _) => base,
        (n, "_") => format!("_{n}"),
        (n, _) => format!("{base}_{n}"),
    }
}

/// Group ATOM records into residues, in file order, keeping only the ones
/// that contribute a letter to the sequence
fn residues(atoms: &[Atom]) -> Vec<Residue> {
    let mut out: Vec<Residue> = vec![];

    for atom in atoms.iter().filter(|a| !a.is_hetatm) {
        match out.last_mut() {
            Some(res) if res.is_same(atom) => res.add(atom),
            _ => {
                let mut res = Residue::new(atom);
                res.add(atom);
                out.push(res);
            }
        }
    }

    out.retain(|r| one_letter(&r.name).is_some());
    out
}

/// Cut the residues into chains by the three rules in the module docs
fn split_chains(residues: &[Residue]) -> Vec<&[Residue]> {
    let mut chains = vec![];
    let mut start = 0;

    for i in 1..residues.len() {
        if is_break(&residues[i - 1], &residues[i]) {
            chains.push(&residues[start..i]);
            start = i;
        }
    }

    if start < residues.len() {
        chains.push(&residues[start..]);
    }

    chains
}

/// Does a new chain start at `cur`?
fn is_break(prev: &Residue, cur: &Residue) -> bool {
    if prev.chain_id != cur.chain_id {
        return true;
    }

    if cur.number < prev.number
        || (cur.number == prev.number && cur.ins_code == prev.ins_code)
    {
        return true;
    }

    // An insertion code (52, 52A, 53) is still running on.
    let runs_on = cur.number == prev.number + 1 || cur.number == prev.number;
    if !runs_on {
        return false;
    }

    match (prev.c, cur.n, prev.ca, cur.ca) {
        (Some(c), Some(n), _, _) => distance(c, n) > MAX_PEPTIDE_BOND,
        (_, _, Some(a), Some(b)) => distance(a, b) > MAX_CA_CA,
        _ => false,
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// One-letter code for a residue name, `None` for anything that is not an
/// amino acid (water, ions, lipids) and for the termini caps
pub fn one_letter(name: &str) -> Option<char> {
    let aa = match name {
        // Biopython 1.87 IUPACData.protein_letters_3to1_extended
        "ALA" => 'A',
        "ARG" => 'R',
        "ASN" => 'N',
        "ASP" => 'D',
        "ASX" => 'B',
        "CYS" => 'C',
        "GLN" => 'Q',
        "GLU" => 'E',
        "GLX" => 'Z',
        "GLY" => 'G',
        "HIS" => 'H',
        "ILE" => 'I',
        "LEU" => 'L',
        "LYS" => 'K',
        "MET" => 'M',
        "PHE" => 'F',
        "PRO" => 'P',
        "PYL" => 'O',
        "SEC" => 'U',
        "SER" => 'S',
        "THR" => 'T',
        "TRP" => 'W',
        "TYR" => 'Y',
        "VAL" => 'V',
        "XAA" => 'X',
        "XLE" => 'J',
        // Protonation and tautomer states
        "ASH" => 'D',
        "GLH" => 'E',
        "HID" | "HIE" | "HIP" | "HSD" | "HSE" | "HSP" => 'H',
        // Cysteine family
        "CYX" | "CYM" | "CYN" | "CME" | "CSO" | "CSS" | "CSX" | "OCS" => 'C',
        // Phosphorylated
        "SEP" => 'S',
        "TPO" => 'T',
        "PTR" => 'Y',
        // Methionine and analogs
        "MSE" | "FME" | "MHO" | "SME" => 'M',
        // Lysine carboxylation, methylation, retinal
        "KCX" | "MLZ" | "MLY" | "M3L" | "KPI" | "LYR" => 'K',
        // Glutamate variants
        "CGU" | "PCA" => 'E',
        // Histidine variants
        "HIC" | "HRG" => 'H',
        // Threonine and tyrosine variants
        "OMT" => 'T',
        "TYI" | "TYS" => 'Y',
        // Generic covalent adduct
        "COV" => 'X',
        // Aminoisobutyric acid, approximated as alanine
        "AIB" => 'A',
        // ACE, NME, NH2, FOR (caps) and everything else
        _ => return None,
    };
    Some(aa)
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// One ATOM line in the fixed columns pdbrust reads
    fn atom(
        serial: i32,
        name: &str,
        res: &str,
        chain: &str,
        num: i32,
        xyz: [f64; 3],
    ) -> String {
        format!(
            "ATOM  {serial:>5} {name:<4} {res:>3} {chain:1}{num:>4}    {:>8.3}{:>8.3}{:>8.3}  1.00  0.00           C  ",
            xyz[0], xyz[1], xyz[2]
        )
    }

    /// A backbone residue (N, CA, C) whose C sits 1.33 A before the next N
    /// when residues are laid 3.8 A apart along x from `x0`
    fn backbone(
        serial: &mut i32,
        res: &str,
        chain: &str,
        num: i32,
        x0: f64,
    ) -> Vec<String> {
        let mut lines = vec![];
        for (name, dx) in [("N", 0.0), ("CA", 1.2), ("C", 2.47)] {
            *serial += 1;
            lines.push(atom(*serial, name, res, chain, num, [x0 + dx, 0.0, 0.0]));
        }
        lines
    }

    fn fasta(lines: &[String]) -> String {
        let text = lines.join("\n") + "\nEND\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.pdb");
        std::fs::write(&path, text).unwrap();
        fasta_from_pdb(&path).unwrap()
    }

    /// Residues `names` numbered from `first`, starting at `x0`
    fn chain(
        serial: &mut i32,
        names: &str,
        chain: &str,
        first: i32,
        x0: f64,
    ) -> Vec<String> {
        names
            .split(' ')
            .enumerate()
            .flat_map(|(i, name)| {
                backbone(serial, name, chain, first + i as i32, x0 + 3.8 * i as f64)
            })
            .collect()
    }

    #[test]
    fn blank_chains_restarting_at_one_are_kept() {
        // MDR00087701's shape: blank IDs, every chain numbered from 1.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA LYS", " ", 1, 0.0);
        lines.extend(chain(&mut s, "GLY SER", " ", 1, 100.0));
        assert_eq!(fasta(&lines), ">_\nMAK\n>_2\nGS\n");
    }

    #[test]
    fn repeated_letter_continuous_numbers_split_on_geometry() {
        // MDR00097110's shape: both chains "A", numbering runs straight on.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA LYS", "A", 1, 0.0);
        lines.push("TER".to_string());
        lines.extend(chain(&mut s, "GLY SER", "A", 4, 100.0));
        assert_eq!(fasta(&lines), ">A\nMAK\n>A_2\nGS\n");
    }

    #[test]
    fn chain_letter_change_splits() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.extend(chain(&mut s, "GLY SER", "B", 3, 7.6));
        assert_eq!(fasta(&lines), ">A\nMA\n>B\nGS\n");
    }

    #[test]
    fn missing_loop_stays_one_chain() {
        // A numbering jump with a gap is a missing loop, not a new chain.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.extend(chain(&mut s, "GLY SER", "A", 20, 60.0));
        assert_eq!(fasta(&lines), ">A\nMAGS\n");
    }

    #[test]
    fn coarse_grained_splits_on_ca_distance() {
        let mut s = 0;
        let mut lines = vec![];
        for (i, res) in ["MET", "ALA", "LYS"].iter().enumerate() {
            s += 1;
            lines.push(atom(
                s,
                "CA",
                res,
                " ",
                i as i32 + 1,
                [3.8 * i as f64, 0.0, 0.0],
            ));
        }
        for (i, res) in ["GLY", "SER"].iter().enumerate() {
            s += 1;
            lines.push(atom(
                s,
                "CA",
                res,
                " ",
                i as i32 + 4,
                [50.0 + 3.8 * i as f64, 0.0, 0.0],
            ));
        }
        assert_eq!(fasta(&lines), ">_\nMAK\n>_2\nGS\n");
    }

    #[test]
    fn hetatm_water_and_caps_are_left_out() {
        let mut s = 0;
        let mut lines = vec![atom(0, "C", "ACE", "A", 0, [-2.0, 0.0, 0.0])];
        lines.extend(chain(&mut s, "MET HID CYX", "A", 1, 0.0));
        lines.push(atom(90, "C", "NME", "A", 4, [12.0, 0.0, 0.0]));
        lines.push(atom(91, "OW", "SOL", "A", 5, [30.0, 0.0, 0.0]));
        lines.push(
            atom(92, "CA", "MSE", "A", 6, [40.0, 0.0, 0.0])
                .replacen("ATOM  ", "HETATM", 1),
        );
        assert_eq!(fasta(&lines), ">A\nMHC\n");
    }

    #[test]
    fn only_the_first_model_is_read() {
        let mut s = 0;
        let body = chain(&mut s, "MET ALA", "A", 1, 0.0);
        let mut lines = vec!["MODEL        1".to_string()];
        lines.extend(body.clone());
        lines.push("ENDMDL".to_string());
        lines.push("MODEL        2".to_string());
        lines.extend(body);
        lines.push("ENDMDL".to_string());
        assert_eq!(fasta(&lines), ">A\nMA\n");
    }

    #[test]
    fn all_x_record_is_dropped() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.extend(chain(&mut s, "XAA", "B", 1, 50.0));
        lines.extend(chain(&mut s, "GLY", "C", 1, 90.0));
        assert_eq!(fasta(&lines), ">A\nMA\n>C\nG\n");
    }

    #[test]
    fn no_protein_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.pdb");
        std::fs::write(&path, atom(1, "OW", "SOL", "A", 1, [0.0; 3]) + "\nEND\n")
            .unwrap();
        assert!(fasta_from_pdb(&path).is_err());
    }
}
