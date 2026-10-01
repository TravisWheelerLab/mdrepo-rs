//! Polymer chains (protein, DNA, RNA), read from a processed `full.pdb`
//!
//! The same chains fill `md_chain`/`md_polymer` and write `sequence.fa`, so the
//! FASTA and the chain rows can never disagree.
//!
//! History: this replaced `get_sequence_from_pdb.py` (2026-09-28), which
//! grouped residues by chain letter with Biopython and so dropped or joined
//! chains whenever the letters were blank or repeated, which is most MD
//! output (MDR00087701: five blank chains each numbered from 1; MDR00097110:
//! two chains both lettered "A"). MDR-94 (2026-10-01) then changed what goes
//! *into* a chain, to agree with the ligand inference in simulation-processing
//! (`mol_id._polymer_residues`) and with MDDB-workflow.
//!
//! # What is in a chain
//!
//! A residue belongs to the chain of the residue before it in the file when
//!
//!   1. its backbone is bonded to that residue's: C(i) to N(i+1), or O3'(i) to
//!      P(i+1), within 1.9 A, whatever either residue is called and whether
//!      it is written as ATOM or HETATM; or
//!   2. both are named amino acids, or both named nucleotides, with the same
//!      chain letter, and the numbering jumps forward: a missing loop stays
//!      one chain (agreed by Travis 2026-09-30); or
//!   3. both are named coarse-grained amino acids (no N or C), the numbering runs
//!      on, and CA(i) to CA(i+1) is within 4.2 A.
//!
//! Rules 2 and 3 join residues that are not bonded, so they need the NAMES to
//! say amino acid or nucleotide (`one_letter`, or a nucleotide name). Atom
//! names are not enough: DDD writes a whole peptide ligand as one residue
//! `LIG` with atoms N, CA, C, ..., and a numbering gap before it would
//! otherwise join it to the protein.
//!
//! A chain letter change always starts a new chain, and so does numbering
//! that goes backwards or repeats unless rule 1 holds. pdbrust and this
//! reader both drop TER records, so TER is not used.
//!
//! Water and single-atom residues (ions) are never in a chain. A run of one
//! residue is a chain only if it is a named amino acid (in `one_letter`), or
//! has a cap on either side (an alanine dipeptide). The named amino acid is
//! kept as the old splitter kept it: most of these are a chain residue
//! stranded by missing neighbours with the numbering running straight on
//! (DDD renumbers from 1, so a missing loop shows no jump: 1aht's hirugen
//! LEU, 2c4w's 163, 167, 168), and dropping them would lose the residue.
//!
//! # What is recorded per chain
//!
//!   - `residues`: each residue's chemical-component code. Force-field names
//!     for protonation states and termini are folded to the parent
//!     (`HIE`/`HSD` -> `HIS`, `CYX` -> `CYS`, `NALA`/`CALA` -> `ALA`,
//!     `DA5` -> `DA`, `RA3` -> `A`), so one protein simulated two ways is one
//!     `md_polymer` row. Anything else keeps its name (`MSE`, `CGU`, `SEP`).
//!   - `sequence`: one letter per residue. A modified residue takes its
//!     parent's letter (`CGU` -> `E`), because that is what aligns to UniProt;
//!     `X` only when there is no parent.
//!   - Caps (`ACE`, `NME`, `NH2`, ...) at either end are recorded as caps, not
//!     residues.
//!
//! # Reading the file
//!
//! Only the first MODEL is read. The residue name is read from columns 17-21
//! and trimmed: cpptraj writes a 4-letter name in 17-20 and GROMACS in 18-21,
//! and the 3-column reading of the PDB standard (18-20, as pdbrust and
//! MDAnalysis do) turns `NALA` into `ALA` but `CYSP` into `YSP`. Column 17 is
//! the alternate-location flag in the standard; processed files never use it.

use anyhow::{Result, anyhow, bail};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    path::Path,
};

/// C(i)-N(i+1) is 1.33 A and O3'(i)-P(i+1) 1.61 A; the same atoms unbonded do
/// not come within 2.5 A. The same cutoff as `mol_id._LINK_CUTOFF`.
const MAX_BACKBONE_BOND: f64 = 1.9;

/// Consecutive CA atoms sit ~3.8 A apart (trans) and ~2.9 A (cis).
const MAX_CA_CA: f64 = 4.2;

const WATER: &[&str] = &[
    "HOH", "WAT", "SOL", "H2O", "DOD", "TIP", "TIP3", "TIP4", "TIP5", "T3P", "T4P",
    "T5P", "SPC", "SPCE", "TP3",
];

/// Caps that start a chain, and caps that end one
const N_CAPS: &[&str] = &["ACE", "FOR"];
const C_CAPS: &[&str] = &["NME", "NH2", "NHE", "NMA", "CT3"];

// --------------------------------------------------
/// What kind of polymer a chain is, as stored in `md_polymer.polymer_type`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolymerType {
    Protein,
    Dna,
    Rna,
}

impl fmt::Display for PolymerType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            PolymerType::Protein => "protein",
            PolymerType::Dna => "dna",
            PolymerType::Rna => "rna",
        })
    }
}

// --------------------------------------------------
/// One polymer molecule in the structure, in file order
#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// The chain letter as written, possibly blank
    pub label: String,
    pub polymer_type: PolymerType,
    /// Chemical-component codes, caps excluded
    pub residues: Vec<String>,
    /// One letter per entry in `residues`
    pub sequence: String,
    /// Residue numbers as written, caps excluded
    pub first_residue: i32,
    pub last_residue: i32,
    pub n_terminal_cap: Option<String>,
    pub c_terminal_cap: Option<String>,
    /// Index of the chain's first atom in the file, for ordering it among
    /// declared chains; not stored
    pub first_atom: usize,
}

// --------------------------------------------------
/// The classes a residue can fall in for chain building
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Water,
    Ion,
    Amino,
    Nucleic { rna: bool },
    Other,
}

// --------------------------------------------------
/// One residue as it appears in the file, with the atoms the rules need
#[derive(Debug)]
struct Residue {
    first_atom: usize,
    heavy: Formula,
    chain_id: String,
    number: i32,
    ins_code: char,
    name: String,
    num_atoms: usize,
    n: Option<[f64; 3]>,
    ca: Option<[f64; 3]>,
    c: Option<[f64; 3]>,
    p: Option<[f64; 3]>,
    o3: Option<[f64; 3]>,
    has_o2: bool,
    has_sugar: bool,
}

impl Residue {
    fn new(index: usize, atom: &PdbAtom) -> Self {
        Residue {
            first_atom: index,
            heavy: Formula::new(),
            chain_id: atom.chain_id.clone(),
            number: atom.res_seq,
            ins_code: atom.ins_code,
            name: atom.res_name.clone(),
            num_atoms: 0,
            n: None,
            ca: None,
            c: None,
            p: None,
            o3: None,
            has_o2: false,
            has_sugar: false,
        }
    }

    fn is_same(&self, atom: &PdbAtom) -> bool {
        self.chain_id == atom.chain_id
            && self.number == atom.res_seq
            && self.ins_code == atom.ins_code
            && self.name == atom.res_name
    }

    fn add(&mut self, atom: &PdbAtom) {
        self.num_atoms += 1;
        if !matches!(atom.element.as_str(), "H" | "D") {
            *self.heavy.entry(atom.element.clone()).or_insert(0) += 1;
        }
        let name = atom.name.replace('*', "'");
        let slot = match name.as_str() {
            "N" => &mut self.n,
            "CA" => &mut self.ca,
            "C" => &mut self.c,
            "P" => &mut self.p,
            "O3'" => &mut self.o3,
            "O2'" => {
                self.has_o2 = true;
                return;
            }
            "C1'" | "C4'" => {
                self.has_sugar = true;
                return;
            }
            _ => return,
        };
        // First one wins
        if slot.is_none() {
            *slot = Some(atom.xyz);
        }
    }

    /// The polymer family from the residue name alone (amino acid or
    /// nucleotide), for the rules that join residues without a bond
    fn named_family(&self) -> Option<u8> {
        if nucleic_by_name(&self.name).is_some() {
            Some(1)
        } else if one_letter(&canonical(&self.name)).is_some() {
            Some(0)
        } else {
            None
        }
    }

    fn kind(&self) -> Kind {
        let name = self.name.as_str();
        if WATER.contains(&name) {
            return Kind::Water;
        }
        if let Some(rna) = nucleic_by_name(name) {
            return Kind::Nucleic {
                rna: rna.unwrap_or(self.has_o2),
            };
        }
        let amino_name = one_letter(&canonical(name)).is_some();
        // A coarse-grained amino acid is one CA bead
        if self.num_atoms == 1 && !(amino_name && self.ca.is_some()) {
            return Kind::Ion;
        }
        if amino_name || (self.n.is_some() && self.ca.is_some() && self.c.is_some()) {
            return Kind::Amino;
        }
        if self.has_sugar && (self.o3.is_some() || self.p.is_some()) {
            return Kind::Nucleic { rna: self.has_o2 };
        }
        Kind::Other
    }
}

// --------------------------------------------------
/// The fields of one ATOM/HETATM record this module uses
#[derive(Debug)]
struct PdbAtom {
    name: String,
    res_name: String,
    chain_id: String,
    res_seq: i32,
    ins_code: char,
    xyz: [f64; 3],
    /// Columns 77-78, or the leading letters of the atom name when blank
    element: String,
}

/// The ATOM and HETATM records of the first MODEL, in file order
fn read_atoms(text: &str) -> Result<Vec<PdbAtom>> {
    let mut atoms = vec![];
    for (i, line) in text.lines().enumerate() {
        if line.starts_with("ENDMDL") {
            break;
        }
        if !(line.starts_with("ATOM  ") || line.starts_with("HETATM")) {
            continue;
        }
        let col = |a: usize, b: usize| line.get(a..b.min(line.len())).unwrap_or("");
        let num = |a: usize, b: usize| -> Result<f64> {
            col(a, b).trim().parse::<f64>().map_err(|e| {
                anyhow!("line {}: columns {}-{}: {e}: {line:?}", i + 1, a + 1, b)
            })
        };
        let res_seq = col(22, 26)
            .trim()
            .parse::<i32>()
            .map_err(|e| anyhow!("line {}: residue number: {e}: {line:?}", i + 1))?;
        let name = col(12, 16).trim().to_uppercase();
        let element = match col(76, 78).trim() {
            "" => name
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .chars()
                .take(1)
                .collect(),
            e => e.to_uppercase(),
        };
        atoms.push(PdbAtom {
            element,
            name,
            res_name: col(16, 21).trim().to_uppercase(),
            chain_id: col(21, 22).to_string(),
            res_seq,
            ins_code: col(26, 27).chars().next().unwrap_or(' '),
            xyz: [num(30, 38)?, num(38, 46)?, num(46, 54)?],
        });
    }
    Ok(atoms)
}

/// Group atoms into residues, in file order
fn residues(atoms: &[PdbAtom]) -> Vec<Residue> {
    let mut out: Vec<Residue> = vec![];
    for (i, atom) in atoms.iter().enumerate() {
        match out.last_mut() {
            Some(res) if res.is_same(atom) => res.add(atom),
            _ => {
                let mut res = Residue::new(i, atom);
                res.add(atom);
                out.push(res);
            }
        }
    }
    out
}

/// Does `cur` continue the chain that `prev` is in? The rules in the module
/// docs.
fn joins(prev: &Residue, cur: &Residue) -> bool {
    if prev.chain_id != cur.chain_id {
        return false;
    }

    let bonded = |a: Option<[f64; 3]>, b: Option<[f64; 3]>| match (a, b) {
        (Some(a), Some(b)) => distance(a, b) <= MAX_BACKBONE_BOND,
        _ => false,
    };
    if bonded(prev.c, cur.n) || bonded(prev.o3, cur.p) {
        return true;
    }

    // Rules 2 and 3 need both residues named as polymer residues
    let (pf, cf) = (prev.named_family(), cur.named_family());
    if pf.is_none() || pf != cf {
        return false;
    }

    let backwards = cur.number < prev.number
        || (cur.number == prev.number && cur.ins_code == prev.ins_code);
    if backwards {
        return false;
    }

    // An insertion code (52, 52A, 53) is still running on
    let runs_on = cur.number == prev.number + 1 || cur.number == prev.number;
    if !runs_on {
        return true;
    }

    // The numbers run on but the backbone was not bonded above. Where the
    // backbone atoms are there, that is a break; where they are not
    // (coarse-grained, CA beads only), fall back to the CA distance; with
    // nothing to measure, the numbering decides.
    if (prev.c.is_some() && cur.n.is_some()) || (prev.o3.is_some() && cur.p.is_some()) {
        return false;
    }
    match (prev.ca, cur.ca) {
        (Some(a), Some(b)) => distance(a, b) <= MAX_CA_CA,
        _ => true,
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

// --------------------------------------------------
/// Read `pdb` and return its polymer chains
pub fn chains_from_pdb(pdb: &Path) -> Result<Vec<Chain>> {
    let text = std::fs::read_to_string(pdb)
        .map_err(|err| anyhow!("{}: {err}", pdb.display()))?;
    chains_from_text(&text).map_err(|err| anyhow!("{}: {err}", pdb.display()))
}

/// The polymer chains in PDB text
pub fn chains_from_text(text: &str) -> Result<Vec<Chain>> {
    Ok(Structure::read(text)?.chains)
}

/// A structure's residues, and the chains made of them
struct Structure {
    residues: Vec<Residue>,
    chains: Vec<Chain>,
    /// For each residue, whether it is in one of `chains` (caps included)
    in_chain: Vec<bool>,
}

impl Structure {
    fn read(text: &str) -> Result<Structure> {
        let residues = residues(&read_atoms(text)?);
        let polymer: Vec<usize> = (0..residues.len())
            .filter(|&i| !matches!(residues[i].kind(), Kind::Water | Kind::Ion))
            .collect();

        let mut runs: Vec<&[usize]> = vec![];
        let mut start = 0;
        for i in 1..polymer.len() {
            if !joins(&residues[polymer[i - 1]], &residues[polymer[i]]) {
                runs.push(&polymer[start..i]);
                start = i;
            }
        }
        if start < polymer.len() {
            runs.push(&polymer[start..]);
        }

        let mut chains = vec![];
        let mut in_chain = vec![false; residues.len()];
        for run in runs {
            let run_residues: Vec<&Residue> =
                run.iter().map(|&i| &residues[i]).collect();
            if let Some(chain) = make_chain(&run_residues) {
                for &i in run {
                    in_chain[i] = true;
                }
                chains.push(chain);
            }
        }

        Ok(Structure {
            residues,
            chains,
            in_chain,
        })
    }
}

/// A chain from one run of joined residues, or `None` if it is not a polymer
fn make_chain(run: &[&Residue]) -> Option<Chain> {
    let mut body = run;
    let mut n_cap = None;
    let mut c_cap = None;
    if let Some(first) = body.first()
        && body.len() > 1
        && N_CAPS.contains(&first.name.as_str())
    {
        n_cap = Some(first.name.clone());
        body = &body[1..];
    }
    if let Some(last) = body.last()
        && body.len() > 1
        && C_CAPS.contains(&last.name.as_str())
    {
        c_cap = Some(last.name.clone());
        body = &body[..body.len() - 1];
    }

    if body.len() == 1
        && n_cap.is_none()
        && c_cap.is_none()
        && one_letter(&canonical(&body[0].name)).is_none()
    {
        return None;
    }

    let (mut amino, mut dna, mut rna) = (0, 0, 0);
    for res in body {
        match res.kind() {
            Kind::Amino => amino += 1,
            Kind::Nucleic { rna: false } => dna += 1,
            Kind::Nucleic { rna: true } => rna += 1,
            _ => {}
        }
    }
    let polymer_type = if amino == 0 && dna == 0 && rna == 0 {
        return None;
    } else if amino >= dna && amino >= rna {
        PolymerType::Protein
    } else if dna >= rna {
        PolymerType::Dna
    } else {
        PolymerType::Rna
    };

    let mut residues = vec![];
    let mut sequence = String::new();
    for res in body {
        let (code, letter) = match polymer_type {
            PolymerType::Protein => {
                let code = canonical(&res.name);
                let letter = one_letter(&code).unwrap_or('X');
                (code, letter)
            }
            PolymerType::Dna | PolymerType::Rna => {
                nucleic_code(&res.name, polymer_type == PolymerType::Rna)
            }
        };
        residues.push(code);
        sequence.push(letter);
    }

    Some(Chain {
        label: body[0].chain_id.clone(),
        polymer_type,
        residues,
        sequence,
        first_residue: body[0].number,
        last_residue: body[body.len() - 1].number,
        n_terminal_cap: n_cap,
        c_terminal_cap: c_cap,
        first_atom: run[0].first_atom,
    })
}

// --------------------------------------------------
/// Heavy atoms by element, e.g. {C: 26, N: 8, O: 7}
pub type Formula = BTreeMap<String, u32>;

/// Show a formula as `C26 N8 O7`
pub fn formula_text(formula: &Formula) -> String {
    formula
        .iter()
        .map(|(e, n)| format!("{}{n}", e[..1].to_string() + &e[1..].to_lowercase()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Heavy atoms of each standard amino acid as a free molecule (C, N, O, S,
/// Se), from the chemical component dictionary
fn amino_acid_heavy_atoms(code: &str) -> Option<[(&'static str, u32); 5]> {
    let (c, n, o, s, se) = match code {
        "ALA" => (3, 1, 2, 0, 0),
        "ARG" => (6, 4, 2, 0, 0),
        "ASN" => (4, 2, 3, 0, 0),
        "ASP" => (4, 1, 4, 0, 0),
        "CYS" => (3, 1, 2, 1, 0),
        "GLN" => (5, 2, 3, 0, 0),
        "GLU" => (5, 1, 4, 0, 0),
        "GLY" => (2, 1, 2, 0, 0),
        "HIS" => (6, 3, 2, 0, 0),
        "ILE" => (6, 1, 2, 0, 0),
        "LEU" => (6, 1, 2, 0, 0),
        "LYS" => (6, 2, 2, 0, 0),
        "MET" => (5, 1, 2, 1, 0),
        "PHE" => (9, 1, 2, 0, 0),
        "PRO" => (5, 1, 2, 0, 0),
        "SER" => (3, 1, 3, 0, 0),
        "THR" => (4, 1, 3, 0, 0),
        "TRP" => (11, 2, 2, 0, 0),
        "TYR" => (9, 1, 3, 0, 0),
        "VAL" => (5, 1, 2, 0, 0),
        "SEC" => (3, 1, 2, 0, 1),
        "PYL" => (12, 3, 3, 0, 0),
        _ => return None,
    };
    Some([("C", c), ("N", n), ("O", o), ("S", s), ("SE", se)])
}

/// The three-letter code for a standard one-letter amino acid
pub fn amino_acid_code(letter: char) -> Option<&'static str> {
    Some(match letter {
        'A' => "ALA",
        'R' => "ARG",
        'N' => "ASN",
        'D' => "ASP",
        'C' => "CYS",
        'Q' => "GLN",
        'E' => "GLU",
        'G' => "GLY",
        'H' => "HIS",
        'I' => "ILE",
        'L' => "LEU",
        'K' => "LYS",
        'M' => "MET",
        'F' => "PHE",
        'P' => "PRO",
        'S' => "SER",
        'T' => "THR",
        'W' => "TRP",
        'Y' => "TYR",
        'V' => "VAL",
        'U' => "SEC",
        'O' => "PYL",
        _ => return None,
    })
}

/// The heavy atoms a peptide of `residues` must have, written as one blob:
/// the free amino acids, less one O for each peptide bond (the water lost
/// there), plus the caps. Fails for a residue whose composition is not known
/// here; DNA and RNA are not handled yet.
pub fn expected_peptide_formula(
    residues: &[String],
    n_cap: Option<&str>,
    c_cap: Option<&str>,
) -> Result<Formula> {
    let mut formula = Formula::new();
    let add = |e: &str, n: i64, formula: &mut Formula| {
        let v = formula.entry(e.to_string()).or_insert(0);
        *v = u32::try_from(i64::from(*v) + n).unwrap_or(0);
    };
    for code in residues {
        let atoms = amino_acid_heavy_atoms(code).ok_or_else(|| {
            anyhow!("no composition known for residue {code}, so it cannot be checked")
        })?;
        for (e, n) in atoms {
            add(e, i64::from(n), &mut formula);
        }
    }
    add("O", -(residues.len() as i64 - 1).max(0), &mut formula);
    // Each cap is joined by an amide bond, which also loses a water
    match n_cap {
        None => {}
        Some("ACE") => {
            add("C", 2, &mut formula);
            add("O", 1, &mut formula);
        }
        Some("FOR") => {
            add("C", 1, &mut formula);
            add("O", 1, &mut formula);
        }
        Some(cap) => bail!("no composition known for the N-terminal cap {cap}"),
    }
    match c_cap {
        None => {}
        Some("NME") | Some("NMA") => {
            add("C", 1, &mut formula);
            add("N", 1, &mut formula);
            add("O", -1, &mut formula);
        }
        Some("NH2") | Some("NHE") => {
            add("N", 1, &mut formula);
            add("O", -1, &mut formula);
        }
        Some(cap) => bail!("no composition known for the C-terminal cap {cap}"),
    }
    formula.retain(|_, n| *n > 0);
    Ok(formula)
}

/// A molecule outside every chain whose heavy atoms match a declared sequence
#[derive(Debug, Clone, PartialEq)]
pub struct Blob {
    /// The residue name it is written under (DDD's `LIG`)
    pub name: String,
    pub chain_label: String,
    /// Index of its first atom in the file
    pub first_atom: usize,
}

/// The molecules in `text` outside every chain whose heavy-atom composition
/// equals `expected`, in file order. A molecule is a run of consecutive
/// residues with the same name and chain letter that are not water, so a
/// residue whose writer split one atom off under another number (1mf4's
/// `O.co`, residue 12 beside residue 120) is still read whole.
pub fn find_blobs(text: &str, expected: &Formula) -> Result<Vec<Blob>> {
    let structure = Structure::read(text)?;
    let mut blobs = vec![];
    let mut i = 0;
    let res = &structure.residues;
    while i < res.len() {
        if structure.in_chain[i] || res[i].kind() == Kind::Water {
            i += 1;
            continue;
        }
        let mut formula = res[i].heavy.clone();
        let mut j = i + 1;
        while j < res.len()
            && !structure.in_chain[j]
            && res[j].name == res[i].name
            && res[j].chain_id == res[i].chain_id
        {
            for (e, n) in &res[j].heavy {
                *formula.entry(e.clone()).or_insert(0) += n;
            }
            j += 1;
        }
        if &formula == expected {
            blobs.push(Blob {
                name: res[i].name.clone(),
                chain_label: res[i].chain_id.clone(),
                first_atom: res[i].first_atom,
            });
        }
        i = j;
    }
    Ok(blobs)
}

// --------------------------------------------------
/// Read `pdb` and return its protein sequences as FASTA text, for BLAST
pub fn fasta_from_pdb(pdb: &Path) -> Result<String> {
    let fasta = fasta_from_chains(&chains_from_pdb(pdb)?);
    if fasta.is_empty() {
        bail!(
            r#"Failed to find sequence in structure "{}""#,
            pdb.display()
        );
    }
    Ok(fasta)
}

/// FASTA text for the protein chains, empty if there are none. DNA and RNA
/// are left out because the sequences go to `blastp`; a chain that is all `X`
/// is left out because it has nothing to align.
pub fn fasta_from_chains(chains: &[Chain]) -> String {
    let mut fasta = String::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for chain in chains {
        if chain.polymer_type == PolymerType::Protein
            && chain.sequence.chars().any(|c| c != 'X')
        {
            let id = header(&chain.label, &mut seen);
            fasta.push_str(&format!(">{id}\n{}\n", chain.sequence));
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

// --------------------------------------------------
/// The chemical-component code for an amino-acid residue name: force-field
/// protonation and terminal names folded to the parent, anything else as is
pub fn canonical(name: &str) -> String {
    let name = name.to_uppercase();
    let parent = match name.as_str() {
        "HID" | "HIE" | "HIP" | "HSD" | "HSE" | "HSP" | "HISD" | "HISE" | "HISH"
        | "HIS1" | "HIS2" => "HIS",
        "CYX" | "CYM" | "CYN" | "CYS2" | "CYSH" => "CYS",
        "ASH" | "ASPH" | "ASPP" => "ASP",
        "GLH" | "GLUH" | "GLUP" => "GLU",
        "LYN" | "LYSH" | "LSN" => "LYS",
        "ARN" => "ARG",
        "TYM" => "TYR",
        _ => {
            // AMBER's terminal residues: NALA, CALA, NHIE, ...
            if name.len() == 4
                && (name.starts_with('N') || name.starts_with('C'))
                && is_standard_amino(&canonical(&name[1..]))
            {
                return canonical(&name[1..]);
            }
            return name;
        }
    };
    parent.to_string()
}

fn is_standard_amino(code: &str) -> bool {
    matches!(
        code,
        "ALA"
            | "ARG"
            | "ASN"
            | "ASP"
            | "CYS"
            | "GLN"
            | "GLU"
            | "GLY"
            | "HIS"
            | "ILE"
            | "LEU"
            | "LYS"
            | "MET"
            | "PHE"
            | "PRO"
            | "SER"
            | "THR"
            | "TRP"
            | "TYR"
            | "VAL"
            | "SEC"
            | "PYL"
    )
}

/// Is `name` a nucleotide, and if the name says, is it RNA? `Some(None)` is a
/// nucleotide whose name does not say (CHARMM's `ADE`, ...).
fn nucleic_by_name(name: &str) -> Option<Option<bool>> {
    let base = name.trim_end_matches(['5', '3', 'N']);
    match base {
        "DA" | "DC" | "DG" | "DT" | "DU" | "DI" => Some(Some(false)),
        "A" | "C" | "G" | "U" | "I" | "RA" | "RC" | "RG" | "RU" => Some(Some(true)),
        "ADE" | "CYT" | "GUA" | "THY" | "URA" => Some(None),
        _ => None,
    }
}

/// (chemical-component code, letter) for a nucleotide in a DNA or RNA chain
fn nucleic_code(name: &str, rna: bool) -> (String, char) {
    let base = name.trim_end_matches(['5', '3', 'N']);
    let letter = match base {
        "DA" | "A" | "RA" | "ADE" => Some('A'),
        "DC" | "C" | "RC" | "CYT" => Some('C'),
        "DG" | "G" | "RG" | "GUA" => Some('G'),
        "DT" | "THY" => Some('T'),
        "DU" | "U" | "RU" | "URA" => Some('U'),
        "DI" | "I" => Some('I'),
        _ => None,
    };
    match letter {
        Some(l) if rna => (l.to_string(), l),
        Some(l) => (format!("D{l}"), l),
        None => (name.to_string(), 'X'),
    }
}

/// One-letter code for an amino-acid residue name, `None` for anything that
/// is not one (water, ions, lipids, nucleotides) and for the termini caps
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

    /// One record with a 3-letter name in columns 18-20 (the PDB standard)
    fn atom(
        serial: i32,
        name: &str,
        res: &str,
        chain: &str,
        num: i32,
        xyz: [f64; 3],
    ) -> String {
        format!(
            "ATOM  {serial:>5} {name:<4} {res:>3} {chain:1}{num:>4}    \
             {:>8.3}{:>8.3}{:>8.3}  1.00  0.00           C  ",
            xyz[0], xyz[1], xyz[2]
        )
    }

    /// The same with a 4-letter name in columns 17-20, as cpptraj writes it
    fn atom4(
        serial: i32,
        name: &str,
        res: &str,
        chain: &str,
        num: i32,
        xyz: [f64; 3],
    ) -> String {
        format!(
            "ATOM  {serial:>5} {name:<4}{res:<4} {chain:1}{num:>4}    \
             {:>8.3}{:>8.3}{:>8.3}  1.00  0.00           C  ",
            xyz[0], xyz[1], xyz[2]
        )
    }

    fn hetatm(line: String) -> String {
        line.replacen("ATOM  ", "HETATM", 1)
    }

    /// A backbone residue (N, CA, C) whose C sits 1.33 A before the next N
    /// when residues are laid 3.8 A apart along x from `x0`
    fn backbone(s: &mut i32, res: &str, chain: &str, num: i32, x0: f64) -> Vec<String> {
        [("N", 0.0), ("CA", 1.2), ("C", 2.47)]
            .iter()
            .map(|(name, dx)| {
                *s += 1;
                atom(*s, name, res, chain, num, [x0 + dx, 0.0, 0.0])
            })
            .collect()
    }

    /// Residues `names` numbered from `first`, starting at `x0`
    fn chain(
        s: &mut i32,
        names: &str,
        chain: &str,
        first: i32,
        x0: f64,
    ) -> Vec<String> {
        names
            .split(' ')
            .enumerate()
            .flat_map(|(i, name)| {
                backbone(s, name, chain, first + i as i32, x0 + 3.8 * i as f64)
            })
            .collect()
    }

    /// A nucleotide (P, C4', C1', O3', and O2' for RNA) whose O3' sits 1.6 A
    /// before the next P when residues are laid 3.8 A apart
    fn nucleotide(s: &mut i32, res: &str, num: i32, x0: f64, rna: bool) -> Vec<String> {
        let mut names = vec![("P", 0.0), ("C4'", 1.0), ("C1'", 1.5), ("O3'", 2.2)];
        if rna {
            names.push(("O2'", 1.8));
        }
        names
            .iter()
            .map(|(name, dx)| {
                *s += 1;
                atom(*s, name, res, "B", num, [x0 + dx, 1.0, 0.0])
            })
            .collect()
    }

    fn chains(lines: &[String]) -> Vec<Chain> {
        chains_from_text(&(lines.join("\n") + "\nEND\n")).unwrap()
    }

    fn fasta(lines: &[String]) -> String {
        fasta_from_chains(&chains(lines))
    }

    fn residues_of(chain: &Chain) -> Vec<&str> {
        chain.residues.iter().map(String::as_str).collect()
    }

    // ---- Chain boundaries, unchanged from 44a6695 ----

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
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sequence, "MAGS");
        assert_eq!((got[0].first_residue, got[0].last_residue), (1, 21));
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
            let x = 50.0 + 3.8 * i as f64;
            lines.push(atom(s, "CA", res, " ", i as i32 + 4, [x, 0.0, 0.0]));
        }
        assert_eq!(fasta(&lines), ">_\nMAK\n>_2\nGS\n");
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
    fn no_protein_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.pdb");
        std::fs::write(&path, atom(1, "OW", "SOL", "A", 1, [0.0; 3]) + "\nEND\n")
            .unwrap();
        assert!(fasta_from_pdb(&path).is_err());
    }

    // ---- What goes into a chain: MDR-94 ----

    #[test]
    fn hetatm_residue_bonded_into_the_chain_is_kept() {
        // Was: an MSE written as HETATM was dropped and its neighbours joined.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET", "A", 1, 0.0);
        lines.extend(backbone(&mut s, "MSE", "A", 2, 3.8).into_iter().map(hetatm));
        lines.extend(chain(&mut s, "ALA", "A", 3, 7.6));
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(residues_of(&got[0]), ["MET", "MSE", "ALA"]);
        assert_eq!(got[0].sequence, "MMA");
    }

    #[test]
    fn unknown_residue_keeps_its_place() {
        // Was: dropped, and MET and ALA joined across the gap as "MA".
        let mut s = 0;
        let lines = chain(&mut s, "MET ZZZ ALA", "A", 1, 0.0);
        let got = chains(&lines);
        assert_eq!(residues_of(&got[0]), ["MET", "ZZZ", "ALA"]);
        assert_eq!(got[0].sequence, "MXA");
        assert_eq!(fasta(&lines), ">A\nMXA\n");
    }

    #[test]
    fn modified_residue_keeps_its_code_and_its_parent_letter() {
        let mut s = 0;
        let got = chains(&chain(&mut s, "LEU CGU CGU", "X", 5, 0.0));
        assert_eq!(residues_of(&got[0]), ["LEU", "CGU", "CGU"]);
        assert_eq!(got[0].sequence, "LEE");
    }

    #[test]
    fn force_field_names_fold_to_the_parent() {
        let mut s = 0;
        let got = chains(&chain(&mut s, "HIE CYX ASH HSD", "A", 1, 0.0));
        assert_eq!(residues_of(&got[0]), ["HIS", "CYS", "ASP", "HIS"]);
        assert_eq!(got[0].sequence, "HCDH");
    }

    #[test]
    fn four_letter_names_are_read_whole() {
        // cpptraj writes 17-20; reading 18-20 gave "ALA" for NALA but "YSP"
        // for CYSP.
        let mut s = 0;
        let mut lines = vec![];
        for (i, res) in ["NALA", "GLY", "CYSP", "CLYS"].iter().enumerate() {
            for (name, dx) in [("N", 0.0), ("CA", 1.2), ("C", 2.47)] {
                s += 1;
                let x = 3.8 * i as f64 + dx;
                lines.push(atom4(s, name, res, "A", i as i32 + 1, [x, 0.0, 0.0]));
            }
        }
        let got = chains(&lines);
        assert_eq!(residues_of(&got[0]), ["ALA", "GLY", "CYSP", "LYS"]);
        assert_eq!(got[0].sequence, "AGXK");
    }

    #[test]
    fn caps_are_recorded_as_caps() {
        // Was: left out, with nothing recorded.
        let mut s = 100;
        let mut lines = vec![
            atom(1, "CH3", "ACE", "A", 0, [-2.5, 0.0, 0.0]),
            atom(2, "C", "ACE", "A", 0, [-1.33, 0.0, 0.0]),
        ];
        lines.extend(chain(&mut s, "MET ALA", "A", 1, 0.0));
        lines.push(atom(3, "N", "NME", "A", 3, [3.8 + 2.47 + 1.33, 0.0, 0.0]));
        lines.push(atom(4, "CH3", "NME", "A", 3, [3.8 + 2.47 + 2.8, 0.0, 0.0]));
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(residues_of(&got[0]), ["MET", "ALA"]);
        assert_eq!(got[0].n_terminal_cap.as_deref(), Some("ACE"));
        assert_eq!(got[0].c_terminal_cap.as_deref(), Some("NME"));
        assert_eq!((got[0].first_residue, got[0].last_residue), (1, 2));
    }

    #[test]
    fn water_ions_and_lone_unknown_residues_are_not_chains() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.push(atom(90, "OW", "SOL", "A", 3, [30.0, 0.0, 0.0]));
        lines.push(atom(91, "OW", "SOL", "A", 3, [30.5, 0.0, 0.0]));
        lines.push(atom(92, "NA", "NA", "A", 4, [40.0, 0.0, 0.0]));
        lines.extend(chain(&mut s, "ZZZ", "C", 1, 90.0));
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sequence, "MA");
    }

    #[test]
    fn a_lone_named_amino_acid_is_still_a_chain() {
        // As before MDR-94: usually a residue stranded by missing neighbours.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.extend(chain(&mut s, "LEU", "A", 3, 40.0));
        assert_eq!(fasta(&lines), ">A\nMA\n>A_2\nL\n");
    }

    #[test]
    fn all_x_chain_is_a_chain_but_not_in_the_fasta() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.extend(chain(&mut s, "XAA XAA", "B", 1, 50.0));
        assert_eq!(chains(&lines).len(), 2);
        assert_eq!(fasta(&lines), ">A\nMA\n");
    }

    #[test]
    fn a_ligand_bonded_to_a_side_chain_is_not_in_the_chain() {
        // A covalent inhibitor on a lysine's NZ is not a backbone bond.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET LYS ALA", "A", 1, 0.0);
        lines.push(atom(90, "NZ", "LYS", "A", 2, [5.0, -4.0, 0.0]));
        lines.push(atom(91, "C", "LIG", "A", 4, [5.0, -5.34, 0.0]));
        lines.push(atom(92, "O", "LIG", "A", 4, [6.0, -5.9, 0.0]));
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(residues_of(&got[0]), ["MET", "LYS", "ALA"]);
    }

    #[test]
    fn wrapped_numbering_stays_one_chain_when_bonded() {
        // Was: 9999 -> 0 read as numbering going backwards, so a new chain.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 9998, 0.0);
        lines.extend(chain(&mut s, "GLY SER", "A", 0, 7.6));
        assert_eq!(fasta(&lines), ">A\nMAGS\n");
    }

    #[test]
    fn dna_and_rna_are_chains_but_not_in_the_fasta() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        for (i, res) in ["DA5", "DT", "DG3"].iter().enumerate() {
            lines.extend(nucleotide(
                &mut s,
                res,
                i as i32 + 1,
                50.0 + 3.8 * i as f64,
                false,
            ));
        }
        for (i, res) in ["RA", "U"].iter().enumerate() {
            let x = 100.0 + 3.8 * i as f64;
            lines.extend(nucleotide(&mut s, res, i as i32 + 1, x, true));
        }
        let got = chains(&lines);
        assert_eq!(got.len(), 3);
        assert_eq!(got[1].polymer_type, PolymerType::Dna);
        assert_eq!(residues_of(&got[1]), ["DA", "DT", "DG"]);
        assert_eq!(got[1].sequence, "ATG");
        assert_eq!(got[2].polymer_type, PolymerType::Rna);
        assert_eq!(residues_of(&got[2]), ["A", "U"]);
        assert_eq!(fasta(&lines), ">A\nMA\n");
    }

    // ---- The fixtures shared with simulation-processing's mol_id tests ----

    fn fixture(name: &str) -> Vec<Chain> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/polymer")
            .join(name);
        chains_from_pdb(&path).unwrap()
    }

    #[test]
    fn gla_domain_cgu_is_in_the_chain_and_vorapaxar_is_not() {
        let got = fixture("gla_chain_namd_frame.pdb");
        assert_eq!(got.len(), 1);
        assert_eq!(residues_of(&got[0]), ["LEU", "CGU", "CGU"]);
        assert_eq!(got[0].sequence, "LEE");
    }

    #[test]
    fn retinal_lysine_is_in_the_chain() {
        let got = fixture("retinal_lysine_peptide.pdb");
        assert_eq!(residues_of(&got[0]), ["ALA", "LYR", "ALA"]);
        assert_eq!(got[0].sequence, "AKA");
    }

    #[test]
    fn palmitoyl_cysteine_is_in_the_chain() {
        let got = fixture("palmitoyl_cysteine_peptide.pdb");
        assert_eq!(residues_of(&got[0]), ["ALA", "CYP", "ALA"]);
        assert_eq!(got[0].sequence, "AXA");
    }

    // ---- Composition: a peptide written as one residue ----

    fn formula(pairs: &[(&str, u32)]) -> Formula {
        pairs.iter().map(|(e, n)| (e.to_string(), *n)).collect()
    }

    fn codes(names: &str) -> Vec<String> {
        names.split(' ').map(String::from).collect()
    }

    #[test]
    fn vafrs_has_the_heavy_atoms_of_1mf4s_lig() {
        // 1mf4's LIG: 41 heavy atoms, counted by hand from the contributor's
        // file, its stray O.co included.
        let got = expected_peptide_formula(&codes("VAL ALA PHE ARG SER"), None, None)
            .unwrap();
        assert_eq!(got, formula(&[("C", 26), ("N", 8), ("O", 7)]));
        assert_eq!(formula_text(&got), "C26 N8 O7");
    }

    #[test]
    fn caps_change_the_formula() {
        let ala2 = codes("ALA ALA");
        let plain = expected_peptide_formula(&ala2, None, None).unwrap();
        assert_eq!(plain, formula(&[("C", 6), ("N", 2), ("O", 3)]));
        let capped = expected_peptide_formula(&ala2, Some("ACE"), Some("NME")).unwrap();
        assert_eq!(capped, formula(&[("C", 9), ("N", 3), ("O", 3)]));
        let amide = expected_peptide_formula(&ala2, None, Some("NH2")).unwrap();
        assert_eq!(amide, formula(&[("C", 6), ("N", 3), ("O", 2)]));
        assert!(expected_peptide_formula(&codes("SEP ALA"), None, None).is_err());
    }

    /// A blob residue: `atoms` as (name, element), all one residue name
    fn blob(
        s: &mut i32,
        res: &str,
        num: i32,
        atoms: &[(&str, &str)],
        x0: f64,
    ) -> Vec<String> {
        atoms
            .iter()
            .enumerate()
            .map(|(i, (name, el))| {
                *s += 1;
                let mut line = atom(*s, name, res, "A", num, [x0 + i as f64, 5.0, 0.0]);
                line.replace_range(76..78, &format!("{el:>2}"));
                line
            })
            .collect()
    }

    #[test]
    fn a_blob_split_across_two_residue_numbers_is_read_whole() {
        // Gly-Gly as one residue, C4 N2 O3, with one O written under another
        // number, as 1mf4's O.co is.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        let atoms = [
            ("N", "N"),
            ("CA", "C"),
            ("C", "C"),
            ("O", "O"),
            ("N1", "N"),
            ("CA1", "C"),
            ("C1", "C"),
            ("O1", "O"),
        ];
        lines.extend(blob(&mut s, "LIG", 120, &atoms, 20.0));
        lines.extend(blob(&mut s, "LIG", 12, &[("O.co", "O")], 30.0));
        let text = lines.join("\n") + "\nEND\n";
        let want = expected_peptide_formula(&codes("GLY GLY"), None, None).unwrap();
        let got = find_blobs(&text, &want).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "LIG");
        // The protein chain is unaffected, and is not a blob candidate
        assert_eq!(chains_from_text(&text).unwrap().len(), 1);
        let none = expected_peptide_formula(&codes("GLY ALA"), None, None).unwrap();
        assert!(find_blobs(&text, &none).unwrap().is_empty());
    }

    #[test]
    fn dna_only_structure_has_chains_and_an_empty_fasta() {
        // The no-protein case: chains for md_chain, nothing for blastp.
        let mut s = 0;
        let mut lines = vec![];
        for (i, res) in ["DA5", "DC", "DG3"].iter().enumerate() {
            lines.extend(nucleotide(&mut s, res, i as i32 + 1, 3.8 * i as f64, false));
        }
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].polymer_type, PolymerType::Dna);
        assert_eq!(fasta_from_chains(&got), "");
    }

    #[test]
    fn a_peptide_blob_after_a_numbering_gap_is_not_joined_to_the_protein() {
        // DDD's LIG has backbone atom names but is not a named amino acid, so
        // the missing-loop rule does not apply to it.
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        lines.extend(chain(&mut s, "LIG", "A", 200, 40.0));
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(residues_of(&got[0]), ["MET", "ALA"]);
    }
}
