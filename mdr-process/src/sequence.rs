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
//! A strand's 5' nucleotide has no P, so there is no O3'-P bond to measure
//! to it. It starts a new chain whatever the numbering says, as does a
//! residue the names or atoms call a terminus: Amber's `DC5`/`DG3`, the
//! 5'/3'-hydroxyl hydrogens `H5T`/`HO5'` and `H3T`/`HO3'`. tleap numbers the
//! two strands of a duplex straight through and cpptraj writes no chain
//! IDs, so without this every Amber duplex read as one chain (1BNA as a
//! 24-nt strand).
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

/// A CRYST1 cell with any edge shorter than this is not a periodic box to
/// measure distances in: GROMACS writes `CRYST1 1.000 1.000 1.000` for no
/// box, and under the minimum image in a cell this small every atom is
/// within a bond of every other. Twice the longest cutoff (`MAX_CA_CA`),
/// rounded up, so that the minimum image is unambiguous for any real cell.
const MIN_CELL_EDGE: f64 = 10.0;

pub(crate) const WATER: &[&str] = &[
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
    /// Where the backbone is broken inside the chain (a missing loop, rule
    /// 2): the index in `residues` of each residue whose backbone is not
    /// bonded to the one before it, though both have the atoms. Only these
    /// places may have residues missing when the chain is matched to a PDB
    /// entry (`reference.rs`); not stored
    pub breaks: Vec<usize>,
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
    /// The 5'-terminal hydroxyl hydrogen (H5T, HO5') and the 3' one (H3T,
    /// HO3'): a strand's ends, by atom
    has_h5t: bool,
    has_h3t: bool,
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
            has_h5t: false,
            has_h3t: false,
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
        // Not hydrogen, and not a massless site: cpptraj writes an Amber
        // extra point (TIP4P-Ew's EPW, `include_ep`) as element XP, and
        // psf_elements leaves a CHARMM lone pair's element blank
        if !matches!(atom.element.as_str(), "H" | "D" | "XP" | "EP" | "LP" | "") {
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
            "H5T" | "HO5'" => {
                self.has_h5t = true;
                return;
            }
            "H3T" | "HO3'" => {
                self.has_h3t = true;
                return;
            }
            _ => return,
        };
        // First one wins
        if slot.is_none() {
            *slot = Some(atom.xyz);
        }
    }

    /// A nucleotide by name or by its sugar
    fn is_nucleotide(&self) -> bool {
        nucleic_by_name(&self.name).is_some() || self.has_sugar
    }

    /// A nucleotide the name or atoms call a strand's 5' end: Amber's
    /// `DA5`/`RG5`, or the 5'-terminal hydroxyl hydrogen
    fn starts_strand(&self) -> bool {
        self.is_nucleotide()
            && (self.has_h5t
                || (nucleic_by_name(&self.name).is_some() && self.name.ends_with('5')))
    }

    /// A nucleotide the name or atoms call a strand's 3' end
    fn ends_strand(&self) -> bool {
        self.is_nucleotide()
            && (self.has_h3t
                || (nucleic_by_name(&self.name).is_some() && self.name.ends_with('3')))
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
/// The periodic cell from CRYST1, as three box vectors: `a` along x, `b` in
/// the xy plane (the GROMACS convention, which cpptraj's PDB writer and
/// MDAnalysis share)
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cell {
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
}

impl Cell {
    /// From the cell lengths (A) and angles (degrees); None for a cell that
    /// is not a box to measure in (`MIN_CELL_EDGE`)
    fn new(lengths: [f64; 3], angles: [f64; 3]) -> Option<Cell> {
        let [la, lb, lc] = lengths;
        if lengths.iter().any(|&l| !l.is_finite() || l < MIN_CELL_EDGE)
            || angles
                .iter()
                .any(|&t| !t.is_finite() || t <= 0.0 || t >= 180.0)
        {
            return None;
        }
        let [alpha, beta, gamma] = angles.map(f64::to_radians);
        let (cos_a, cos_b, cos_g, sin_g) =
            (alpha.cos(), beta.cos(), gamma.cos(), gamma.sin());
        let cx = lc * cos_b;
        let cy = lc * (cos_a - cos_b * cos_g) / sin_g;
        let cz2 = lc * lc - cx * cx - cy * cy;
        if cz2 <= 0.0 {
            return None;
        }
        Some(Cell {
            a: [la, 0.0, 0.0],
            b: [lb * cos_g, lb * sin_g, 0.0],
            c: [cx, cy, cz2.sqrt()],
        })
    }

    /// The first CRYST1 record in `text`, if it describes a box
    fn read(text: &str) -> Option<Cell> {
        let line = text.lines().find(|l| l.starts_with("CRYST1"))?;
        let field = |a: usize, b: usize| -> Option<f64> {
            line.get(a..b.min(line.len()))?.trim().parse::<f64>().ok()
        };
        Cell::new(
            [field(6, 15)?, field(15, 24)?, field(24, 33)?],
            [field(33, 40)?, field(40, 47)?, field(47, 54)?],
        )
    }

    /// `v` in cell coordinates
    fn fractional(&self, v: [f64; 3]) -> [f64; 3] {
        // The vectors are lower triangular, so back-substitute
        let sc = v[2] / self.c[2];
        let sb = (v[1] - sc * self.c[1]) / self.b[1];
        let sa = (v[0] - sb * self.b[0] - sc * self.c[0]) / self.a[0];
        [sa, sb, sc]
    }

    fn cartesian(&self, s: [f64; 3]) -> [f64; 3] {
        let mut v = [0.0; 3];
        for (k, vec) in [self.a, self.b, self.c].iter().enumerate() {
            for i in 0..3 {
                v[i] += s[k] * vec[i];
            }
        }
        v
    }

    /// The length of the shortest periodic image of the vector from `a` to
    /// `b`. The image nearest in cell coordinates and its 26 neighbours are
    /// tried, which finds the true minimum in any cell GROMACS or Amber
    /// writes (both keep the cell reduced).
    fn min_image_distance(&self, a: [f64; 3], b: [f64; 3]) -> f64 {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let s = self.fractional(d).map(|x| x - x.round());
        let mut best = f64::INFINITY;
        for na in [-1.0, 0.0, 1.0] {
            for nb in [-1.0, 0.0, 1.0] {
                for nc in [-1.0, 0.0, 1.0] {
                    let v = self.cartesian([s[0] + na, s[1] + nb, s[2] + nc]);
                    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    best = best.min(len);
                }
            }
        }
        best
    }
}

/// The distance from `a` to `b`, under the minimum image when there is a
/// cell: cpptraj's autoimage and GROMACS's `-pbc atom` both leave molecules
/// broken across the box, and a chain's C-N bond then spans the cell.
fn distance_in(cell: Option<&Cell>, a: [f64; 3], b: [f64; 3]) -> f64 {
    match cell {
        Some(cell) => cell.min_image_distance(a, b),
        None => distance(a, b),
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
            // A massless site (Amber EP1/EPW, CHARMM LP1/LPA) has no element
            "" if name.starts_with("EP") || name.starts_with("LP") => String::new(),
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
            // Trimmed: a blank chain ID is stored as "", not " " (md_chain)
            chain_id: col(21, 22).trim().to_string(),
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
fn joins(cell: Option<&Cell>, prev: &Residue, cur: &Residue) -> bool {
    if prev.chain_id != cur.chain_id {
        return false;
    }

    if backbone_bonded(cell, prev, cur) {
        return true;
    }

    // A strand ends where the names or atoms say so, and where a nucleotide
    // without a P (a 5' end) follows one with an O3': there is no bond to
    // measure, and the numbering must not decide, because tleap numbers a
    // duplex's strands straight through.
    if prev.ends_strand() || cur.starts_strand() {
        return false;
    }
    if prev.o3.is_some() && cur.p.is_none() && cur.is_nucleotide() {
        return false;
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
    if backbone_present(prev, cur) {
        return false;
    }
    match (prev.ca, cur.ca) {
        (Some(a), Some(b)) => distance_in(cell, a, b) <= MAX_CA_CA,
        _ => true,
    }
}

/// C(i) to N(i+1), or O3'(i) to P(i+1), within `MAX_BACKBONE_BOND`
fn backbone_bonded(cell: Option<&Cell>, prev: &Residue, cur: &Residue) -> bool {
    let bonded = |a: Option<[f64; 3]>, b: Option<[f64; 3]>| match (a, b) {
        (Some(a), Some(b)) => distance_in(cell, a, b) <= MAX_BACKBONE_BOND,
        _ => false,
    };
    bonded(prev.c, cur.n) || bonded(prev.o3, cur.p)
}

/// Both residues have the atoms of a backbone bond, so whether they are
/// bonded can be measured
fn backbone_present(prev: &Residue, cur: &Residue) -> bool {
    (prev.c.is_some() && cur.n.is_some()) || (prev.o3.is_some() && cur.p.is_some())
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
    /// For each residue, the index in `chains` of the chain it is in (caps
    /// included)
    chain_of: Vec<Option<usize>>,
}

impl Structure {
    fn read(text: &str) -> Result<Structure> {
        let cell = Cell::read(text);
        let residues = residues(&read_atoms(text)?);
        let polymer: Vec<usize> = (0..residues.len())
            .filter(|&i| !matches!(residues[i].kind(), Kind::Water | Kind::Ion))
            .collect();

        let mut runs: Vec<&[usize]> = vec![];
        let mut start = 0;
        for i in 1..polymer.len() {
            if !joins(
                cell.as_ref(),
                &residues[polymer[i - 1]],
                &residues[polymer[i]],
            ) {
                runs.push(&polymer[start..i]);
                start = i;
            }
        }
        if start < polymer.len() {
            runs.push(&polymer[start..]);
        }

        let mut chains = vec![];
        let mut in_chain = vec![false; residues.len()];
        let mut chain_of = vec![None; residues.len()];
        for run in runs {
            let run_residues: Vec<&Residue> =
                run.iter().map(|&i| &residues[i]).collect();
            if let Some(chain) = make_chain(cell.as_ref(), &run_residues) {
                for &i in run {
                    in_chain[i] = true;
                    chain_of[i] = Some(chains.len());
                }
                chains.push(chain);
            }
        }

        Ok(Structure {
            residues,
            chains,
            in_chain,
            chain_of,
        })
    }
}

/// A chain from one run of joined residues, or `None` if it is not a polymer
fn make_chain(cell: Option<&Cell>, run: &[&Residue]) -> Option<Chain> {
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

    let breaks = (1..body.len())
        .filter(|&i| {
            backbone_present(body[i - 1], body[i])
                && !backbone_bonded(cell, body[i - 1], body[i])
        })
        .collect();

    Some(Chain {
        label: body[0].chain_id.clone(),
        polymer_type,
        residues,
        sequence,
        first_residue: body[0].number,
        last_residue: body[body.len() - 1].number,
        n_terminal_cap: n_cap,
        c_terminal_cap: c_cap,
        breaks,
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
        // The 22 standard amino acids, else the CCD table (decision 4(b),
        // residue_formulas.rs); a code in neither is refused
        let atoms: Vec<(&str, u32)> = match amino_acid_heavy_atoms(code) {
            Some(atoms) => atoms.to_vec(),
            None => crate::residue_formulas::component_heavy_atoms(code)
                .ok_or_else(|| {
                    anyhow!(
                        "no composition known for residue {code}, so it cannot be checked \
                         (add it with `mdr-process ccd-formula {code}`)"
                    )
                })?
                .iter()
                .map(|(e, n)| (e.as_str(), *n))
                .collect(),
        };
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

/// Heavy atoms of each standard nucleotide as the chemical component
/// dictionary has it, 5' phosphate included (C, N, O, P): DNA's DA..DT and
/// RNA's A, C, G, U
fn nucleotide_heavy_atoms(code: &str) -> Option<[(&'static str, u32); 4]> {
    let (c, n, o, p) = match code {
        "DA" => (10, 5, 6, 1),
        "DC" => (9, 3, 7, 1),
        "DG" => (10, 5, 7, 1),
        "DT" => (10, 2, 8, 1),
        "DU" => (9, 2, 8, 1),
        "A" => (10, 5, 7, 1),
        "C" => (9, 3, 8, 1),
        "G" => (10, 5, 8, 1),
        "U" => (9, 2, 9, 1),
        _ => return None,
    };
    Some([("C", c), ("N", n), ("O", o), ("P", p)])
}

/// The heavy atoms a DNA or RNA strand of `residues` (component codes, 5' to
/// 3') can have when written as one residue (DDD's `LIG`), most likely first:
/// every nucleotide's heavy atoms, minus one O per phosphodiester bond; then
/// the same without the 5' phosphate (one P and three O fewer), which PDB
/// entries often leave off (3bbb, 4nku, 6kdi; 2026-11-04). Non-standard
/// nucleotides (RPC, 64T, ...) come from the CCD table, as for peptides.
pub fn expected_nucleic_formulas(residues: &[String]) -> Result<Vec<Formula>> {
    if residues.is_empty() {
        bail!("a nucleic-acid ligand needs at least one nucleotide");
    }
    let mut formula = Formula::new();
    for code in residues {
        let atoms: Vec<(&str, u32)> = match nucleotide_heavy_atoms(code) {
            Some(atoms) => atoms.to_vec(),
            None => crate::residue_formulas::component_heavy_atoms(code)
                .ok_or_else(|| {
                    anyhow!(
                        "no composition known for nucleotide {code}, so it cannot be checked \
                         (add it with `mdr-process ccd-formula {code}`)"
                    )
                })?
                .iter()
                .map(|(e, n)| (e.as_str(), *n))
                .collect(),
        };
        for (e, n) in atoms {
            *formula.entry(e.to_string()).or_insert(0) += n;
        }
    }
    let links = residues.len() as u32 - 1;
    let o = formula.get("O").copied().unwrap_or(0);
    if o < links {
        bail!(
            "{} has too few oxygens for {links} phosphodiester bonds",
            residues.join("-")
        );
    }
    formula.insert("O".to_string(), o - links);
    formula.retain(|_, n| *n > 0);

    let mut out = vec![formula.clone()];
    let (p, o) = (
        formula.get("P").copied().unwrap_or(0),
        formula.get("O").copied().unwrap_or(0),
    );
    if p >= 1 && o >= 3 {
        let mut bare = formula;
        bare.insert("P".to_string(), p - 1);
        bare.insert("O".to_string(), o - 3);
        bare.retain(|_, n| *n > 0);
        out.push(bare);
    }
    Ok(out)
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
/// One residue of a structure as the splitter sees it, for writers that
/// need every residue (the mmCIF writer): where its atoms are, the chain
/// it is in (an index into the chains returned with it; caps included),
/// and whether it is water or a one-atom ion
#[derive(Debug, Clone, PartialEq)]
pub struct ResidueInfo {
    pub first_atom: usize,
    pub num_atoms: usize,
    pub chain: Option<usize>,
    pub water: bool,
    pub ion: bool,
}

/// The chains of PDB text, and every residue (`ResidueInfo`) in file order
pub fn residue_layout(text: &str) -> Result<(Vec<Chain>, Vec<ResidueInfo>)> {
    let structure = Structure::read(text)?;
    let residues = structure
        .residues
        .iter()
        .zip(&structure.chain_of)
        .map(|(res, chain)| {
            let kind = res.kind();
            ResidueInfo {
                first_atom: res.first_atom,
                num_atoms: res.num_atoms,
                chain: *chain,
                water: kind == Kind::Water,
                ion: kind == Kind::Ion && chain.is_none(),
            }
        })
        .collect();
    Ok((structure.chains, residues))
}

// --------------------------------------------------
/// The chain IDs given when a file is relettered, in order
pub const CHAIN_LETTERS: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// What `assign_chain_ids` did with a PDB's text
#[derive(Debug, PartialEq)]
pub enum Labeled {
    /// Every chain already has its own letter: left as it is
    Kept,
    /// No polymer chain, so nothing to letter
    NoChains,
    /// More chains than `CHAIN_LETTERS`: left as it is
    TooManyChains(usize),
    /// More than one MODEL: left as it is (processed files have one)
    SeveralModels,
    /// The text with each chain's atoms lettered A, B, ... in file order
    Done { text: String, chains: usize },
}

/// MDR-106: the one rule for the chain IDs of the PDBs we publish, whatever
/// wrote them (GROMACS, cpptraj for Amber, cpptraj/parmed for NAMD/CHARMM).
///
/// The chains are the ones this module finds, so the file and `md_chain`
/// agree (they are read with the same rules). Their IDs are kept when each
/// chain already has its own letter: one ID per chain (the splitter
/// guarantees that), a letter A-Z or a-z, and no two chains the same (DDD's
/// and the PDB's own IDs stay). Otherwise every chain is relettered A, B, ...
/// in file order (Ken, 2026-11-03): a blank column; mdCATH's "0"; GPCRmd's
/// "P" for every protein chain.
///
/// When relettering, an atom outside every chain (ligand, ion, water,
/// lipid) keeps its ID unless that is now one of the chain letters, and is
/// then blank (Ken, 2026-11-03): GPCRmd's W, M and I stay beside chains A
/// and B.
///
/// Only column 22 changes, on ATOM/HETATM records and on a TER record (the
/// ID of the atom before it): atom order and count do not, so the
/// trajectories still match. Line endings are kept as they are.
pub fn assign_chain_ids(text: &str) -> Result<Labeled> {
    if text
        .lines()
        .filter(|l| l.starts_with("MODEL "))
        .nth(1)
        .is_some()
    {
        return Ok(Labeled::SeveralModels);
    }
    let atoms = read_atoms(text)?;
    let structure = Structure::read(text)?;
    let num_chains = structure.chains.len();
    if num_chains == 0 {
        return Ok(Labeled::NoChains);
    }

    // Is every chain's ID already its own letter?
    let ids: Vec<&str> = structure.chains.iter().map(|c| c.label.as_str()).collect();
    let own_letter =
        |id: &str| id.len() == 1 && id.chars().all(|c| c.is_ascii_alphabetic());
    let distinct: std::collections::HashSet<&str> = ids.iter().copied().collect();
    if ids.iter().all(|id| own_letter(id)) && distinct.len() == ids.len() {
        return Ok(Labeled::Kept);
    }

    let letters: Vec<char> = CHAIN_LETTERS.chars().collect();
    if num_chains > letters.len() {
        return Ok(Labeled::TooManyChains(num_chains));
    }
    let used = &letters[..num_chains];

    // Each atom's ID: its chain's letter, or its own unless that is now a
    // chain's
    let mut atom_id: Vec<char> = atoms
        .iter()
        .map(|a| match a.chain_id.chars().next() {
            Some(c) if !used.contains(&c) => c,
            _ => ' ',
        })
        .collect();
    for (res, chain) in structure.residues.iter().zip(&structure.chain_of) {
        if let Some(c) = chain {
            for slot in &mut atom_id[res.first_atom..res.first_atom + res.num_atoms] {
                *slot = letters[*c];
            }
        }
    }

    // `line` with column 22 set to `id`, unless it is already
    let set_column = |line: &str, id: char| -> Result<String> {
        let (body, end) = match line.strip_suffix("\r\n") {
            Some(b) => (b, "\r\n"),
            None => match line.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (line, ""),
            },
        };
        let current = body
            .get(21..22)
            .and_then(|c| c.chars().next())
            .unwrap_or(' ');
        if current == id {
            return Ok(line.to_string());
        }
        if !body.is_char_boundary(21.min(body.len()))
            || !body.is_char_boundary(22.min(body.len()))
        {
            bail!("not ASCII before column 23: {body:?}");
        }
        let mut out = String::with_capacity(body.len().max(22) + end.len());
        if body.len() < 22 {
            out.push_str(&format!("{body:<21}"));
            out.push(id);
        } else {
            out.push_str(&body[..21]);
            out.push(id);
            out.push_str(&body[22..]);
        }
        out.push_str(end);
        Ok(out)
    };

    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut last: Option<char> = None;
    let mut in_model = true;
    for line in text.split_inclusive('\n') {
        if line.starts_with("ENDMDL") {
            in_model = false;
        }
        if in_model && (line.starts_with("ATOM  ") || line.starts_with("HETATM")) {
            let id = atom_id[i];
            i += 1;
            last = Some(id);
            out.push_str(&set_column(line, id)?);
        } else if in_model && line.starts_with("TER") {
            // TER carries the ID of the chain it ends, where it has the
            // column at all (cpptraj writes it; a bare "TER" has none)
            match last {
                Some(id) if line.trim_end().len() >= 22 => {
                    out.push_str(&set_column(line, id)?)
                }
                _ => out.push_str(line),
            }
            last = None;
        } else {
            out.push_str(line);
        }
    }
    if i != atoms.len() {
        bail!("read {} atoms but rewrote {i}", atoms.len());
    }
    Ok(Labeled::Done {
        text: out,
        chains: num_chains,
    })
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
    // Amber, CHARMM, GROMOS and OPLS names, from each force field's residue
    // database (the GROMACS `aminoacids.rtp` files and Amber's `leaprc`s)
    let parent = match name.as_str() {
        "HID" | "HIE" | "HIP" | "HSD" | "HSE" | "HSP" | "HISD" | "HISE" | "HISH"
        | "HIS1" | "HIS2" | "HISA" | "HISB" => "HIS",
        "CYX" | "CYM" | "CYN" | "CYS1" | "CYS2" | "CYSH" => "CYS",
        "ASH" | "ASPH" | "ASPP" => "ASP",
        "ASN1" => "ASN",
        "GLH" | "GLUH" | "GLUP" => "GLU",
        "LYN" | "LYSH" | "LYSN" | "LSN" => "LYS",
        "ARN" | "ARGN" => "ARG",
        "TYM" => "TYR",
        // Amber phosaa: a phosphorylated residue at either charge is the
        // chemical component
        "S1P" | "S2P" => "SEP",
        "T1P" | "T2P" => "TPO",
        "Y1P" | "Y2P" => "PTR",
        // GROMOS pyroglutamate
        "PGLU" => "PCA",
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
        // Cysteine oxidation
        "CSD" => 'C',
        // Lysine carboxylation, methylation, acetylation, retinal
        "KCX" | "MLZ" | "MLY" | "M3L" | "KPI" | "LYR" | "ALY" => 'K',
        // Glutamate variants
        "CGU" | "PCA" => 'E',
        // Histidine variants
        "HIC" => 'H',
        // Homoarginine is an arginine (its CCD parent), not a histidine
        "HRG" => 'R',
        // Methionine sulfone (its CCD parent is MET), not a threonine
        "OMT" => 'M',
        // Hydroxyproline, norleucine, sarcosine: each its CCD parent
        "HYP" => 'P',
        "NLE" => 'L',
        "SAR" => 'G',
        // Tyrosine variants
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

    /// Residues with 4-letter names, bonded, numbered from 1
    fn chain4(names: &str) -> Vec<String> {
        let mut s = 0;
        let mut lines = vec![];
        for (i, res) in names.split(' ').enumerate() {
            for (name, dx) in [("N", 0.0), ("CA", 1.2), ("C", 2.47)] {
                s += 1;
                let x = 3.8 * i as f64 + dx;
                lines.push(atom4(s, name, res, "A", i as i32 + 1, [x, 0.0, 0.0]));
            }
        }
        lines
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

    /// A strand as tleap writes it: `names` 3.8 A apart from `x0`, numbered
    /// from `first`, the first residue without a P. Blank chain ID.
    fn strand(s: &mut i32, names: &str, first: i32, x0: f64) -> Vec<String> {
        let mut lines = vec![];
        for (i, res) in names.split(' ').enumerate() {
            let x = x0 + 3.8 * i as f64;
            for (name, dx) in [("P", 0.0), ("C4'", 1.0), ("C1'", 1.5), ("O3'", 2.2)] {
                if i == 0 && name == "P" {
                    continue;
                }
                *s += 1;
                lines.push(atom(
                    *s,
                    name,
                    res,
                    " ",
                    first + i as i32,
                    [x + dx, 1.0, 0.0],
                ));
            }
        }
        lines
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

    // ---- MDR-106: letters for blank chain columns ----

    /// Three blank chains numbered from 1 with no TER between them
    /// (MDR00036804's shape), then a ligand and a water, then a TER
    fn three_blank_chains() -> Vec<String> {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA LYS", " ", 1, 0.0);
        lines.extend(chain(&mut s, "GLY SER", " ", 1, 100.0));
        lines.extend(chain(&mut s, "PRO TRP", " ", 1, 200.0));
        s += 1;
        lines.push(atom(s, "C1", "LIG", " ", 1, [300.0, 0.0, 0.0]));
        s += 1;
        lines.push(atom(s, "O", "HOH", " ", 2, [400.0, 0.0, 0.0]));
        lines.push("TER".to_string());
        lines.push("END".to_string());
        lines
    }

    fn column_22(text: &str) -> Vec<char> {
        text.lines()
            .filter(|l| l.starts_with("ATOM  ") || l.starts_with("HETATM"))
            .map(|l| l.chars().nth(21).unwrap())
            .collect()
    }

    #[test]
    fn blank_chains_are_lettered_in_file_order() {
        let text = three_blank_chains().join("\n") + "\n";
        let Labeled::Done { text: got, chains } = assign_chain_ids(&text).unwrap()
        else {
            panic!("not labeled");
        };
        assert_eq!(chains, 3);
        let col = column_22(&got);
        assert_eq!(col.iter().filter(|&&c| c == 'A').count(), 9);
        assert_eq!(col.iter().filter(|&&c| c == 'B').count(), 6);
        assert_eq!(col.iter().filter(|&&c| c == 'C').count(), 6);
        // The ligand and the water stay blank
        assert_eq!(&col[21..], &[' ', ' ']);
        // Only column 22 changed, and the line count is the same
        assert_eq!(got.len(), text.len());
        for (a, b) in text.lines().zip(got.lines()).filter(|(a, _)| a.len() > 22) {
            assert_eq!(a.len(), b.len());
            assert_eq!(a[..21], b[..21]);
            assert_eq!(a[22..], b[22..]);
        }
        // Read back: the same chains, now with their letters
        let before = chains_from_text(&text).unwrap();
        let after = chains_from_text(&got).unwrap();
        assert_eq!(
            after.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(b.residues, a.residues);
            assert_eq!(b.first_residue, a.first_residue);
        }
        // A second pass leaves the file alone
        assert_eq!(assign_chain_ids(&got).unwrap(), Labeled::Kept);
    }

    #[test]
    fn one_blank_chain_is_a() {
        let mut s = 0;
        let text = chain(&mut s, "MET ALA LYS", " ", 1, 0.0).join("\n");
        let Labeled::Done { text: got, chains } = assign_chain_ids(&text).unwrap()
        else {
            panic!("not labeled");
        };
        assert_eq!(chains, 1);
        assert!(column_22(&got).iter().all(|&c| c == 'A'));
        // No final newline in, none out
        assert!(!got.ends_with('\n'));
    }

    #[test]
    fn chains_with_their_own_letters_are_kept() {
        // DDD's and the PDB's IDs: one letter per chain, none repeated; the
        // ligand sharing chain X's letter and the blank water do not matter
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA LYS", "X", 1, 0.0);
        lines.extend(chain(&mut s, "GLY SER", "Y", 1, 100.0));
        s += 1;
        lines.push(atom(s, "C1", "LIG", "X", 9, [300.0, 0.0, 0.0]));
        s += 1;
        lines.push(atom(s, "O", "HOH", " ", 10, [400.0, 0.0, 0.0]));
        assert_eq!(assign_chain_ids(&lines.join("\n")).unwrap(), Labeled::Kept);
    }

    #[test]
    fn a_letter_shared_by_two_chains_is_relettered_and_other_ids_kept() {
        // GPCRmd's shape: two protein chains both "P", then water "W", ions
        // "I", and a lipid "A" that collides with the new chain A
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA LYS", "P", 1, 0.0);
        lines.extend(chain(&mut s, "GLY SER", "P", 1, 100.0));
        for (res, id, x) in [
            ("HOH", "W", 300.0),
            ("SOD", "I", 400.0),
            ("LIP", "A", 500.0),
        ] {
            s += 1;
            lines.push(atom(s, "O", res, id, 1, [x, 0.0, 0.0]));
        }
        let text = lines.join("\n");
        let Labeled::Done { text: got, chains } = assign_chain_ids(&text).unwrap()
        else {
            panic!("not relettered");
        };
        assert_eq!(chains, 2);
        let col = column_22(&got);
        assert!(col[..9].iter().all(|&c| c == 'A'));
        assert!(col[9..15].iter().all(|&c| c == 'B'));
        assert_eq!(&col[15..], &['W', 'I', ' ']);
        assert_eq!(assign_chain_ids(&got).unwrap(), Labeled::Kept);
    }

    #[test]
    fn a_digit_is_not_a_letter() {
        // mdCATH writes chain "0": relettered A (Ken, 2026-11-03)
        let mut s = 0;
        let text = chain(&mut s, "MET ALA LYS", "0", 1, 0.0).join("\n");
        let Labeled::Done { text: got, .. } = assign_chain_ids(&text).unwrap() else {
            panic!("not relettered");
        };
        assert!(column_22(&got).iter().all(|&c| c == 'A'));
    }

    #[test]
    fn ter_takes_the_letter_of_the_chain_it_ends_and_crlf_is_kept() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", " ", 1, 0.0);
        lines.push(format!("TER    {:>4}      ALA     2", s + 1));
        lines.extend(chain(&mut s, "GLY SER", " ", 1, 100.0));
        lines.push("TER".to_string());
        let text = lines.join("\r\n") + "\r\n";
        let Labeled::Done { text: got, .. } = assign_chain_ids(&text).unwrap() else {
            panic!("not labeled");
        };
        let ters: Vec<&str> =
            got.split("\r\n").filter(|l| l.starts_with("TER")).collect();
        assert_eq!(ters[0].chars().nth(21), Some('A'));
        assert_eq!(ters[1], "TER");
        assert_eq!(got.matches("\r\n").count(), text.matches("\r\n").count());
        assert!(!got.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn too_many_chains_or_models_are_left_alone() {
        let mut s = 0;
        let mut lines = vec![];
        for i in 0..63 {
            lines.extend(chain(&mut s, "ALA", " ", i + 1, 100.0 * i as f64));
        }
        assert_eq!(
            assign_chain_ids(&lines.join("\n")).unwrap(),
            Labeled::TooManyChains(63)
        );

        let mut s = 0;
        let model = chain(&mut s, "MET ALA", " ", 1, 0.0).join("\n");
        let text = format!("MODEL 1\n{model}\nENDMDL\nMODEL 2\n{model}\nENDMDL\n");
        assert_eq!(assign_chain_ids(&text).unwrap(), Labeled::SeveralModels);
    }

    #[test]
    fn no_polymer_no_letters() {
        let text = atom(1, "O", "HOH", " ", 1, [0.0, 0.0, 0.0]);
        assert_eq!(assign_chain_ids(&text).unwrap(), Labeled::NoChains);
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
    fn blank_chain_label_is_empty_not_a_space() {
        // MDR00105204's shape: one chain with no chain ID.
        let mut s = 0;
        let lines = chain(&mut s, "MET ALA LYS", " ", 1, 0.0);
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "");
        assert_eq!(fasta(&lines), ">_\nMAK\n");
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
        // The break is recorded before GLY, the first residue after the loop
        assert_eq!(got[0].breaks, vec![2]);
    }

    #[test]
    fn a_bonded_chain_has_no_breaks() {
        let mut s = 0;
        let got = chains(&chain(&mut s, "MET ALA GLY SER", "A", 1, 0.0));
        assert_eq!(got[0].breaks, Vec::<usize>::new());
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

    /// `lines` with residues numbered `from` and up shifted by `by`
    fn shifted(lines: &[String], from: i32, by: [f64; 3]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                let num: i32 = l[22..26].trim().parse().unwrap();
                if num < from {
                    return l.clone();
                }
                let xyz: Vec<f64> = [30, 38, 46]
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| l[c..c + 8].trim().parse::<f64>().unwrap() + by[i])
                    .collect();
                format!(
                    "{}{:>8.3}{:>8.3}{:>8.3}{}",
                    &l[..30],
                    xyz[0],
                    xyz[1],
                    xyz[2],
                    &l[54..]
                )
            })
            .collect()
    }

    fn cryst1(lengths: [f64; 3], angles: [f64; 3]) -> String {
        format!(
            "CRYST1{:>9.3}{:>9.3}{:>9.3}{:>7.2}{:>7.2}{:>7.2} P 1           1",
            lengths[0], lengths[1], lengths[2], angles[0], angles[1], angles[2]
        )
    }

    #[test]
    fn a_chain_wrapped_across_the_box_is_one_chain() {
        // Atoms wrapped into the cell, as mdrun and cpptraj autoimage leave
        // them: residues 3 and 4 sit one box vector away from 1 and 2, so
        // the C-N bond between 2 and 3 spans the box. Without CRYST1 the
        // chain is cut there; with it, MDAnalysis finds one fragment and so
        // does this.
        let mut s = 0;
        let body = chain(&mut s, "MET ALA GLY SER", "A", 1, 0.0);
        let wrapped = shifted(&body, 3, [-40.0, 0.0, 0.0]);
        assert_eq!(chains(&wrapped).len(), 2);

        let mut lines = vec![cryst1([40.0, 40.0, 40.0], [90.0, 90.0, 90.0])];
        lines.extend(wrapped.clone());
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sequence, "MAGS");
        assert_eq!(got[0].breaks, Vec::<usize>::new());

        // A truncated octahedron (the Amber default), with the tail moved by
        // its third box vector, worked out by hand from the cell: c = 40 A at
        // alpha = beta = gamma = 109.47 deg gives (-13.333, -18.856, 32.660).
        let mut lines = vec![cryst1([40.0; 3], [109.4712206; 3])];
        lines.extend(shifted(&body, 3, [-13.333, -18.856, 32.660]));
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sequence, "MAGS");

        // The same shift and no cell: two chains
        assert_eq!(
            chains(&shifted(&body, 3, [-13.333, -18.856, 32.660])).len(),
            2
        );
    }

    #[test]
    fn a_placeholder_cell_is_not_a_box() {
        // GROMACS writes CRYST1 1 1 1 for a system with no box. Under the
        // minimum image in that cell every atom is within a bond of every
        // other, so two chains 100 A apart would be one.
        let mut s = 0;
        let mut lines = vec![cryst1([1.0, 1.0, 1.0], [90.0, 90.0, 90.0])];
        lines.extend(chain(&mut s, "MET ALA", " ", 1, 0.0));
        lines.extend(chain(&mut s, "GLY SER", " ", 1, 100.0));
        assert_eq!(chains(&lines).len(), 2);
        assert_eq!(Cell::read(&lines.join("\n")), None);
    }

    #[test]
    fn minimum_image_matches_a_wider_search() {
        // The nearest image and its 26 neighbours, against every image
        // within four cells (a skewed cell puts the nearest image several
        // cells from the origin), for the cell shapes the engines write and
        // some they do not
        let cells = [
            ([40.0, 40.0, 40.0], [90.0, 90.0, 90.0]),
            ([40.0, 40.0, 40.0], [109.4712206; 3]),
            ([40.0, 40.0, 28.28], [60.0, 60.0, 90.0]),
            ([35.0, 48.0, 61.0], [80.0, 95.0, 110.0]),
        ];
        let mut seed = 12345u64;
        let mut rand = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for (lengths, angles) in cells {
            let cell = Cell::new(lengths, angles).unwrap();
            for _ in 0..200 {
                let a = [
                    rand() * 60.0 - 30.0,
                    rand() * 60.0 - 30.0,
                    rand() * 60.0 - 30.0,
                ];
                let b = [
                    rand() * 60.0 - 30.0,
                    rand() * 60.0 - 30.0,
                    rand() * 60.0 - 30.0,
                ];
                let mut best = f64::INFINITY;
                for na in -4..=4 {
                    for nb in -4..=4 {
                        for nc in -4..=4 {
                            let shift =
                                cell.cartesian([na as f64, nb as f64, nc as f64]);
                            let d = distance(
                                a,
                                [b[0] + shift[0], b[1] + shift[1], b[2] + shift[2]],
                            );
                            best = best.min(d);
                        }
                    }
                }
                let got = cell.min_image_distance(a, b);
                assert!(
                    (got - best).abs() < 1e-9,
                    "{lengths:?} {angles:?}: {got} vs {best}"
                );
            }
        }
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
    fn gromos_and_opls_names_fold_to_the_parent() {
        // GROMOS 54a7 and OPLS-AA as pdb2gmx writes them (aminoacids.rtp):
        // every histidine tautomer, the neutral arginine and lysine, the
        // bridged cysteines, pyroglutamate. These came out `X` with their own
        // md_polymer key, so a GROMOS protein with more than three histidines
        // lost its declared UniProt reference (3 mismatches allowed).
        let names = "HISA HISB HISH HIS1 HIS2 ARGN LYSH LYSN CYS1 CYS2 CYSH ASN1 PGLU";
        let got = chains(&chain4(names));
        assert_eq!(
            residues_of(&got[0]),
            [
                "HIS", "HIS", "HIS", "HIS", "HIS", "ARG", "LYS", "LYS", "CYS", "CYS",
                "CYS", "ASN", "PCA"
            ]
        );
        assert_eq!(got[0].sequence, "HHHHHRKKCCCNE");
    }

    #[test]
    fn amber_phospho_names_are_the_chemical_component() {
        // phosaa10/14/19 name a phosphoserine S1P or S2P by its charge; the
        // chemical component is SEP either way (TPO, PTR likewise).
        let mut s = 0;
        let got = chains(&chain(&mut s, "S1P S2P T1P T2P Y1P Y2P", "A", 1, 0.0));
        assert_eq!(
            residues_of(&got[0]),
            ["SEP", "SEP", "TPO", "TPO", "PTR", "PTR"]
        );
        assert_eq!(got[0].sequence, "SSTTYY");
    }

    #[test]
    fn parent_letters_follow_the_chemical_component_dictionary() {
        // Each modified residue's `mon_nstd_parent_comp_id` in the CCD: HRG
        // (homoarginine) is ARG and OMT (methionine sulfone) is MET, which the
        // old table had as H and T, copied from get_sequence_from_pdb.py's
        // mislabelled VARIANT table.
        for (code, letter) in [
            ("HRG", 'R'),
            ("OMT", 'M'),
            ("HYP", 'P'),
            ("NLE", 'L'),
            ("ALY", 'K'),
            ("CSD", 'C'),
            ("SAR", 'G'),
            ("HIC", 'H'),
            ("MSE", 'M'),
            ("SEP", 'S'),
        ] {
            assert_eq!(one_letter(code), Some(letter), "{code}");
        }
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
    fn massless_sites_are_not_heavy_atoms() {
        // A chlorobenzene with CGenFF's halogen lone pair, written by cpptraj
        // with include_ep (element XP), by psf_elements (blank) and by a
        // writer with no element column at all
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA", "A", 1, 0.0);
        let heavy = |l: &str, e: &str| {
            let l = format!("{:<76}{e:>2}", &l[..76.min(l.len())]);
            l
        };
        for (name, element) in [
            ("C1", "C"),
            ("CL1", "CL"),
            ("LP1", "XP"),
            ("LP2", ""),
            ("EP1", ""),
        ] {
            s += 1;
            let line = hetatm(atom(s, name, "LIG", "A", 9, [50.0, 0.0, 0.0]));
            let line = if element.is_empty() && name == "EP1" {
                line[..76].to_string()
            } else {
                heavy(&line, element)
            };
            lines.push(line);
        }
        let structure = Structure::read(&(lines.join("\n") + "\nEND\n")).unwrap();
        let ligand = structure.residues.iter().find(|r| r.name == "LIG").unwrap();
        assert_eq!(ligand.heavy, formula(&[("C", 1), ("CL", 1)]));
        assert_eq!(ligand.num_atoms, 5);
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
        // A code outside the CCD table is still refused
        assert!(expected_peptide_formula(&codes("ZZQ ALA"), None, None).is_err());
    }

    #[test]
    fn non_standard_residues_come_from_the_ccd_table() {
        // Phosphoserine: C3 N1 O6 P1 free, + Ala C3 N1 O2, minus one O
        let got = expected_peptide_formula(&codes("SEP ALA"), None, None).unwrap();
        assert_eq!(got, formula(&[("C", 6), ("N", 2), ("O", 7), ("P", 1)]));
        // 2ql5's inhibitor (ACE)DMQ(ASJ): the aldehyde Asp at the C-terminus
        let got =
            expected_peptide_formula(&codes("ASP MET GLN ASJ"), Some("ACE"), None)
                .unwrap();
        assert_eq!(got, formula(&[("C", 20), ("N", 5), ("O", 10), ("S", 1)]));
    }

    /// A formula written as DDD's sort wrote it, e.g. `C22N5O9P1`
    fn lig(text: &str) -> Formula {
        let mut out = Formula::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            let mut element = c.to_string();
            while chars.peek().is_some_and(|c| c.is_ascii_lowercase()) {
                element.push(chars.next().unwrap());
            }
            let mut count = String::new();
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                count.push(chars.next().unwrap());
            }
            out.insert(element.to_uppercase(), count.parse().unwrap_or(1));
        }
        out
    }

    /// Real DDD group B bundles (2026-11-04): LIG's heavy atoms, read from
    /// each tarball's Pro_lig.pdb, against RCSB's sequence for the peptide
    /// entity. Each is (bundle, LIG, residues, N cap, C cap).
    /// (bundle, LIG formula, residues, N cap, C cap)
    type DddCase = (
        &'static str,
        &'static str,
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
    );

    const DDD_NON_STANDARD: &[DddCase] = &[
        ("1o9d", "C26N6O13P1", "GLN SER TYR TPO VAL", None, None),
        ("1a1b", "C26N3O9P1", "PTR GLU DIP", Some("ACE"), None),
        ("3s8o", "C22N5O9P1", "PTR 02K ASN", Some("ACE"), Some("NH2")),
        ("3ove", "C23N5O9P1", "ACT PTR 03E ASN", None, Some("NH2")),
        ("3vb4", "C24N6O6", "BOC ALA VAL LEU 0JU", None, None),
        (
            "4dfw",
            "C36N8O12P1",
            "0LF LEU HIS SER TPO",
            Some("ACE"),
            Some("NH2"),
        ),
        (
            "1fkn",
            "C41N8O14",
            "GLU VAL ASN 1OL ALA GLU PHE",
            None,
            None,
        ),
        (
            "4x9v",
            "C48N8O12P1",
            "4KY LEU 56A SER TPO",
            None,
            Some("NH2"),
        ),
        (
            "5fot",
            "C25N7O6",
            "PHE HIS THR ABA",
            Some("ACE"),
            Some("NH2"),
        ),
        ("1b3h", "C21N5O4", "LYS ALC LYS", None, None),
        (
            "1jm4",
            "C60N26O16S1",
            "SER TYR GLY ARG ALY LYS ARG ARG GLN ARG CYS",
            None,
            None,
        ),
        ("1f1j", "C20N4O11", "ASP GLU VAL ASJ", Some("ACE"), None),
        ("1b4h", "C16N6O4", "LYS DAB LYS", None, None),
        (
            "3rq7",
            "C40N8O11P1",
            "PRO LEU 56A SER TPO",
            Some("ACE"),
            Some("NH2"),
        ),
        (
            "1njt",
            "C23F3N5O8",
            "VAL DMK DMH ALA CFT",
            Some("ACE"),
            None,
        ),
        (
            "3ft2",
            "C45N11O17",
            "VAL LEU CIR ASP ASP LEU LEU GLU ALA",
            None,
            None,
        ),
        ("4whl", "C38N7O10P1", "KAC 56A SER TPO", None, Some("NH2")),
    ];

    #[test]
    fn ddd_bundles_with_non_standard_residues_match_their_lig() {
        for (bundle, want, residues, n_cap, c_cap) in DDD_NON_STANDARD {
            let got = expected_peptide_formula(&codes(residues), *n_cap, *c_cap)
                .unwrap_or_else(|e| panic!("{bundle}: {e}"));
            assert_eq!(got, lig(want), "{bundle}");
        }
    }

    #[test]
    fn ddd_bundles_miss_by_one_residue_dropped() {
        // The check is exact: drop any one residue and none of them matches
        for (bundle, want, residues, n_cap, c_cap) in DDD_NON_STANDARD {
            let all = codes(residues);
            for skip in 0..all.len() {
                let fewer: Vec<String> = all
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != skip)
                    .map(|(_, c)| c.clone())
                    .collect();
                if fewer.is_empty() {
                    continue;
                }
                let got = expected_peptide_formula(&fewer, *n_cap, *c_cap).unwrap();
                assert_ne!(got, lig(want), "{bundle} without residue {skip}");
            }
        }
    }

    #[test]
    fn ddd_bundles_miss_without_their_caps() {
        for (bundle, want, residues, n_cap, c_cap) in DDD_NON_STANDARD {
            if n_cap.is_none() && c_cap.is_none() {
                continue;
            }
            let got = expected_peptide_formula(&codes(residues), None, None).unwrap();
            assert_ne!(got, lig(want), "{bundle} without caps");
        }
    }

    #[test]
    fn one_non_standard_residue_alone_is_its_own_formula() {
        // No peptide bond, so no water lost: SEP free is C3 N1 O6 P1
        let got = expected_peptide_formula(&codes("SEP"), None, None).unwrap();
        assert_eq!(got, formula(&[("C", 3), ("N", 1), ("O", 6), ("P", 1)]));
    }

    #[test]
    fn non_standard_and_standard_residues_mix() {
        // TPO + GLY: C4 N1 O6 P1 + C2 N1 O2, minus one O
        let got = expected_peptide_formula(&codes("TPO GLY"), None, None).unwrap();
        assert_eq!(got, formula(&[("C", 6), ("N", 2), ("O", 7), ("P", 1)]));
        // Order does not change composition
        let back = expected_peptide_formula(&codes("GLY TPO"), None, None).unwrap();
        assert_eq!(got, back);
    }

    #[test]
    fn a_non_standard_residue_takes_both_caps() {
        // (ACE)(SEP)(NH2): SEP C3 N1 O6 P1, ACE +C2 +O1, NH2 +N1 -O1
        let got =
            expected_peptide_formula(&codes("SEP"), Some("ACE"), Some("NH2")).unwrap();
        assert_eq!(got, formula(&[("C", 5), ("N", 2), ("O", 6), ("P", 1)]));
    }

    #[test]
    fn fluorine_and_other_elements_come_through_the_table() {
        // CFT (trifluoromethyl, C H F3) brings F into the formula
        let got = expected_peptide_formula(&codes("ALA CFT"), None, None).unwrap();
        assert_eq!(got.get("F"), Some(&3));
    }

    #[test]
    fn an_unknown_code_is_refused_and_says_how_to_add_it() {
        let err = expected_peptide_formula(&codes("ALA ZZQ"), None, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ZZQ"), "{msg}");
        assert!(msg.contains("mdr-process ccd-formula ZZQ"), "{msg}");
    }

    #[test]
    fn an_unknown_code_anywhere_refuses_the_whole_sequence() {
        for seq in ["ZZQ ALA GLY", "ALA ZZQ GLY", "ALA GLY ZZQ"] {
            assert!(
                expected_peptide_formula(&codes(seq), None, None).is_err(),
                "{seq}"
            );
        }
    }

    #[test]
    fn standard_residues_still_come_from_sequence_rs_not_the_table() {
        // ALA is not in the table, and still works
        assert!(crate::residue_formulas::component_heavy_atoms("ALA").is_none());
        let got = expected_peptide_formula(&codes("ALA"), None, None).unwrap();
        assert_eq!(got, formula(&[("C", 3), ("N", 1), ("O", 2)]));
    }

    #[test]
    fn a_blob_with_a_non_standard_residue_is_found_by_composition() {
        // SEP-ALA written as one LIG residue: C6 N2 O7 P1
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA GLY", "A", 1, 0.0);
        let mut atoms = vec![];
        for (el, n) in [("C", 6), ("N", 2), ("O", 7), ("P", 1)] {
            for i in 0..n {
                atoms.push((format!("{el}{i}"), el));
            }
        }
        let atoms: Vec<(&str, &str)> =
            atoms.iter().map(|(a, e)| (a.as_str(), *e)).collect();
        lines.extend(blob(&mut s, "LIG", 200, &atoms, 40.0));
        let text = lines.join("\n") + "\nEND\n";
        let want = expected_peptide_formula(&codes("SEP ALA"), None, None).unwrap();
        let got = find_blobs(&text, &want).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "LIG");
        // Threonine's phospho form has one C more: not this blob
        let other = expected_peptide_formula(&codes("TPO ALA"), None, None).unwrap();
        assert!(find_blobs(&text, &other).unwrap().is_empty());
    }

    // ---- nucleic acids written as one residue ----

    #[test]
    fn each_standard_nucleotide_alone_is_its_ccd_formula() {
        // The CCD's formulas, hydrogen left out; one residue has no bond
        for (code, c, n, o) in [
            ("DA", 10, 5, 6),
            ("DC", 9, 3, 7),
            ("DG", 10, 5, 7),
            ("DT", 10, 2, 8),
            ("A", 10, 5, 7),
            ("C", 9, 3, 8),
            ("G", 10, 5, 8),
            ("U", 9, 2, 9),
        ] {
            let got = expected_nucleic_formulas(&codes(code)).unwrap();
            assert_eq!(
                got[0],
                formula(&[("C", c), ("N", n), ("O", o), ("P", 1)]),
                "{code}"
            );
        }
    }

    #[test]
    fn each_phosphodiester_bond_loses_one_oxygen() {
        let one = expected_nucleic_formulas(&codes("DA")).unwrap()[0].clone();
        let two = expected_nucleic_formulas(&codes("DA DA")).unwrap()[0].clone();
        let three = expected_nucleic_formulas(&codes("DA DA DA")).unwrap()[0].clone();
        assert_eq!(two["O"], 2 * one["O"] - 1);
        assert_eq!(three["O"], 3 * one["O"] - 2);
        assert_eq!(three["P"], 3);
    }

    #[test]
    fn the_second_candidate_has_no_5_prime_phosphate() {
        let got = expected_nucleic_formulas(&codes("DA DG")).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0]["P"] - 1, got[1]["P"]);
        assert_eq!(got[0]["O"] - 3, got[1]["O"]);
        assert_eq!(got[0]["C"], got[1]["C"]);
        assert_eq!(got[0]["N"], got[1]["N"]);
    }

    #[test]
    fn a_lone_nucleotide_without_its_phosphate_drops_p_entirely() {
        let got = expected_nucleic_formulas(&codes("DA")).unwrap();
        assert_eq!(got[1], formula(&[("C", 10), ("N", 5), ("O", 3)]));
        assert!(!got[1].contains_key("P"));
    }

    #[test]
    fn dna_and_rna_letters_are_different_molecules() {
        // ACG as DNA and as RNA: ribose has one O more per nucleotide
        let dna = expected_nucleic_formulas(&codes("DA DC DG")).unwrap()[0].clone();
        let rna = expected_nucleic_formulas(&codes("A C G")).unwrap()[0].clone();
        assert_eq!(rna["O"], dna["O"] + 3);
        assert_eq!(rna["C"], dna["C"]);
    }

    #[test]
    fn order_does_not_change_a_strands_composition() {
        let a = expected_nucleic_formulas(&codes("DA DG DT")).unwrap();
        let b = expected_nucleic_formulas(&codes("DT DA DG")).unwrap();
        assert_eq!(a, b);
    }

    /// The six DDD group B bundles whose LIG is DNA or RNA (2026-11-04): the
    /// LIG formula read from each tarball, the strand, and which candidate
    /// it is (0: with the 5' phosphate, 1: without)
    const DDD_NUCLEIC: &[(&str, &str, &str, usize)] = &[
        ("3bbb", "C20N10O9P1", "DA DG", 1),
        ("4i67", "C39N18O32P5", "G G G RPC", 0),
        ("4nku", "C19N7O12P1", "A U", 1),
        ("5o58", "C20N10O14P2", "A G", 0),
        ("6kdi", "C20N4O12P1", "64T 5PY", 1),
        ("6p7q", "C20N10O13P2", "A A", 0),
    ];

    #[test]
    fn ddd_nucleic_acid_bundles_match_their_lig() {
        for (bundle, want, residues, which) in DDD_NUCLEIC {
            let got = expected_nucleic_formulas(&codes(residues))
                .unwrap_or_else(|e| panic!("{bundle}: {e}"));
            assert_eq!(got[*which], lig(want), "{bundle}");
            assert_ne!(
                got[1 - *which],
                lig(want),
                "{bundle}: only one candidate fits"
            );
        }
    }

    #[test]
    fn ddd_nucleic_acid_bundles_miss_with_a_nucleotide_dropped_or_swapped() {
        for (bundle, want, residues, _) in DDD_NUCLEIC {
            let all = codes(residues);
            for skip in 0..all.len() {
                let fewer: Vec<String> = all
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != skip)
                    .map(|(_, c)| c.clone())
                    .collect();
                if fewer.is_empty() {
                    continue;
                }
                for f in expected_nucleic_formulas(&fewer).unwrap() {
                    assert_ne!(f, lig(want), "{bundle} without {skip}");
                }
            }
        }
        // 5o58 is A-G; as A-A (6p7q's strand) it must not match
        for f in expected_nucleic_formulas(&codes("A A")).unwrap() {
            assert_ne!(f, lig("C20N10O14P2"));
        }
    }

    #[test]
    fn non_standard_nucleotides_come_from_the_table() {
        // RPC (C9 H15 N3 O11 P2) is not a standard nucleotide
        let got = expected_nucleic_formulas(&codes("RPC")).unwrap();
        assert_eq!(got[0], formula(&[("C", 9), ("N", 3), ("O", 11), ("P", 2)]));
    }

    #[test]
    fn an_unknown_nucleotide_is_refused_and_says_how_to_add_it() {
        let msg = expected_nucleic_formulas(&codes("DA ZZQ"))
            .unwrap_err()
            .to_string();
        assert!(msg.contains("nucleotide ZZQ"), "{msg}");
        assert!(msg.contains("mdr-process ccd-formula ZZQ"), "{msg}");
    }

    #[test]
    fn an_empty_strand_is_refused() {
        assert!(expected_nucleic_formulas(&[]).is_err());
    }

    #[test]
    fn a_dna_blob_is_found_by_composition_with_or_without_its_phosphate() {
        let mut s = 0;
        let mut lines = chain(&mut s, "MET ALA GLY", "A", 1, 0.0);
        // 3bbb's LIG: DA-DG with no 5' phosphate, C20 N10 O9 P1
        let mut atoms = vec![];
        for (el, n) in [("C", 20), ("N", 10), ("O", 9), ("P", 1)] {
            for i in 0..n {
                atoms.push((format!("{el}{i}"), el));
            }
        }
        let atoms: Vec<(&str, &str)> =
            atoms.iter().map(|(a, e)| (a.as_str(), *e)).collect();
        lines.extend(blob(&mut s, "LIG", 300, &atoms, 50.0));
        let text = lines.join("\n") + "\nEND\n";
        let candidates = expected_nucleic_formulas(&codes("DA DG")).unwrap();
        assert!(find_blobs(&text, &candidates[0]).unwrap().is_empty());
        assert_eq!(find_blobs(&text, &candidates[1]).unwrap().len(), 1);
        // DA-DC is another molecule
        for f in expected_nucleic_formulas(&codes("DA DC")).unwrap() {
            assert!(find_blobs(&text, &f).unwrap().is_empty());
        }
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
    fn duplex_strands_numbered_straight_through_are_two_chains() {
        // 1BNA through tleap and cpptraj: blank IDs, residues 1-24, the 5'
        // residue of each strand without a P. MDAnalysis finds two bonded
        // fragments; this found one 24-nt chain. The second strand starts
        // 20 A away, as in a real duplex, so no O3'-P bond spans the gap.
        let mut s = 0;
        let mut lines = strand(&mut s, "DC5 DG DC DG3", 1, 0.0);
        lines.extend(strand(&mut s, "DC5 DG DC DG3", 5, 20.0));
        let got = chains(&lines);
        assert_eq!(got.len(), 2);
        for chain in &got {
            assert_eq!(chain.polymer_type, PolymerType::Dna);
            assert_eq!(chain.sequence, "CGCG");
        }
        assert_eq!((got[1].first_residue, got[1].last_residue), (5, 8));
    }

    #[test]
    fn a_strand_without_terminal_names_still_ends_at_the_missing_p() {
        // CHARMM and the PDB name every nucleotide alike (ADE/CYT, DA/DC);
        // only the missing P and the H5T/HO5' hydrogen mark the 5' end.
        let mut s = 0;
        let mut lines = strand(&mut s, "DC DG DC DG", 1, 0.0);
        lines.extend(strand(&mut s, "DC DG DC DG", 5, 20.0));
        assert_eq!(chains(&lines).len(), 2);

        // With a P on the 5' residue (a 5'-phosphate), the hydrogen alone
        // says where the strand starts
        let mut s = 0;
        let mut lines = vec![];
        for (i, res) in ["DC", "DG", "DC", "DG"].iter().enumerate() {
            let x = 3.8 * i as f64 + if i >= 2 { 20.0 } else { 0.0 };
            lines.extend(nucleotide(&mut s, res, i as i32 + 1, x, false));
            if i == 2 {
                s += 1;
                lines.push(atom(s, "H5T", res, "B", 3, [x - 0.5, 1.0, 0.0]));
            }
        }
        let got = chains(&lines);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].sequence, "CG");
        assert_eq!(got[1].sequence, "CG");
    }

    #[test]
    fn a_strand_with_a_missing_nucleotide_stays_one_chain() {
        // Rule 2 still applies inside a strand: the residue after the gap
        // has its P, so it is not a 5' end.
        let mut s = 0;
        let mut lines = strand(&mut s, "DC5 DG", 1, 0.0);
        for (i, res) in ["DC", "DG3"].iter().enumerate() {
            lines.extend(nucleotide(
                &mut s,
                res,
                i as i32 + 5,
                30.0 + 3.8 * i as f64,
                false,
            ));
        }
        // nucleotide() writes chain B; strand() writes blank
        let lines: Vec<String> = lines
            .iter()
            .map(|l| format!("{} {}", &l[..21], &l[22..]))
            .collect();
        let got = chains(&lines);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].sequence, "CGCG");
        assert_eq!(got[0].breaks, vec![2]);
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
