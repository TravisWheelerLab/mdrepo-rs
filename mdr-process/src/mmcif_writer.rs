//! The standard mmCIF: `full.mmcif` beside `full.pdb`, `minimal.mmcif`
//! beside `minimal.pdb` (MDR-110; plan PLAN-2026-11-03-mmcif-writer.md,
//! Ken's answers 2026-11-03).
//!
//! The same atoms as the pdb beside it, in the same order with the same
//! coordinates, so the trajectories match either file. Added to what a pdb
//! can hold:
//!
//!   - entities: one per polymer (same polymer type, caps and residues = the
//!     same entity, so a homodimer's chains share one), one per ligand
//!     residue name, one for water, one per ion name;
//!   - chains: `auth_asym_id` is the pdb's chain ID, so Mol* colors as
//!     with the pdb; a blank one outside every chain is WAT, ION or HET,
//!     the psf's segments; `label_asym_id` is one per polymer
//!     chain, one per other molecule (residue), one for all water, one per
//!     ion name, as the PDB does;
//!   - the chain table's sequences (`_entity_poly`, `_entity_poly_seq`;
//!     caps included, as the PDB lists them);
//!   - UniProt references (`_struct_ref`, `_struct_ref_seq`);
//!   - ligand identity: name, SMILES, InChIKey (`_pdbx_entity_nonpoly`,
//!     `_pdbx_chem_comp_descriptor`), where the caller knows the residue
//!     name of a ligand;
//!   - the bonds a reader cannot work out from residue names: between two
//!     residues, other than a chain's own backbone (`_struct_conn`:
//!     disulfides, a ligand on the protein, ...). The full bond graph is
//!     the psf's.
//!
//! No simulation metadata. The chains are the splitter's
//! (`sequence::residue_layout`), so the file and md_chain agree.
//!
//! Each file written is read back: the atom count, and every atom's name,
//! residue name and coordinates must be the pdb's, or it is removed.

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    fs,
    path::Path,
};

use crate::{
    psf_writer,
    sequence::{self, PolymerType},
};

/// One UniProt range on a chain: positions in the chain's residues (caps
/// excluded, 1-based) and in the UniProt sequence, as md_polymer_uniprot
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct UniprotRef {
    pub accession: String,
    pub query_start: i32,
    pub query_end: i32,
    pub reference_start: i32,
    pub reference_end: i32,
}

/// A ligand whose residue name in the pdb is known
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LigandInfo {
    pub resname: String,
    pub name: String,
    pub smiles: Option<String>,
    pub inchikey: Option<String>,
}

/// What the writer needs besides the pdb
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct MmcifInput {
    /// `_entry.id`: the MDR accession, e.g. MDR00105918
    pub entry_id: String,
    /// `_struct.title`
    pub title: String,
    /// UniProt ranges per chain, by the chain's index among the splitter's
    /// chains (missing or empty: none)
    pub references: Vec<Vec<UniprotRef>>,
    pub ligands: Vec<LigandInfo>,
}

/// What a written mmCIF holds
#[derive(Debug, PartialEq)]
pub struct MmcifCounts {
    pub num_atoms: usize,
    pub num_entities: usize,
    pub num_chains: usize,
    pub num_links: usize,
    /// Residues of each non-polymer, non-water residue name: a ligand's
    /// copies (mdrepo-rs#105). Ions included, though their copies share one
    /// asym per ion type. A molecule split over several residues counts once
    /// per residue.
    pub residues_by_comp: BTreeMap<String, usize>,
}

// --------------------------------------------------
/// One ATOM/HETATM record, every column the mmCIF needs
#[derive(Debug, Clone)]
struct Atom {
    hetatm: bool,
    name: String,
    resname: String,
    chain: String,
    resseq: String,
    icode: String,
    xyz: [f64; 3],
    occupancy: f64,
    bfactor: f64,
    element: String,
    charge: Option<i32>,
}

/// The ATOM/HETATM records of the first model
fn read_atoms(text: &str) -> Vec<Atom> {
    let mut atoms = vec![];
    for line in text.lines() {
        if line.starts_with("ENDMDL") {
            break;
        }
        let hetatm = line.starts_with("HETATM");
        if !(hetatm || line.starts_with("ATOM  ")) || !line.is_ascii() {
            continue;
        }
        let col =
            |a: usize, b: usize| line.get(a..b.min(line.len())).unwrap_or("").trim();
        let num = |a: usize, b: usize, d: f64| col(a, b).parse::<f64>().unwrap_or(d);
        let name = col(12, 16).to_string();
        let charge = {
            let c = col(78, 80);
            let (digits, sign) = if c.ends_with('-') || c.ends_with('+') {
                (&c[..c.len() - 1], if c.ends_with('-') { -1 } else { 1 })
            } else {
                (c, 1)
            };
            digits.parse::<i32>().ok().map(|n| n * sign)
        };
        atoms.push(Atom {
            hetatm,
            element: psf_writer::element_of(col(76, 78), &name),
            name,
            // cpptraj right-aligns a four-letter name in columns 17-20
            resname: col(16, 21).to_string(),
            chain: col(21, 22).to_string(),
            resseq: col(22, 26).to_string(),
            icode: col(26, 27).to_string(),
            xyz: [num(30, 38, 0.0), num(38, 46, 0.0), num(46, 54, 0.0)],
            occupancy: num(54, 60, 1.0),
            bfactor: num(60, 66, 0.0),
            charge,
        });
    }
    atoms
}

/// The CRYST1 box: lengths and angles
fn read_cell(text: &str) -> Option<([f64; 3], [f64; 3])> {
    let line = text.lines().find(|l| l.starts_with("CRYST1"))?;
    let num =
        |a: usize, b: usize| line.get(a..b.min(line.len()))?.trim().parse::<f64>().ok();
    Some((
        [num(6, 15)?, num(15, 24)?, num(24, 33)?],
        [num(33, 40)?, num(40, 47)?, num(47, 54)?],
    ))
}

// --------------------------------------------------
/// A CIF value: `?` for nothing, quoted when it must be
fn cif(value: &str) -> String {
    if value.is_empty() {
        return "?".to_string();
    }
    let lower = value.to_ascii_lowercase();
    // A quote anywhere is quoted too, as the PDB writes "O5'"
    let needs_quote = value
        .chars()
        .any(|c| c.is_whitespace() || c == '\'' || c == '"')
        || value.starts_with(['_', '#', '$', '\'', '"', ';', '[', ']'])
        || ["data_", "loop_", "save_", "global_", "stop_"]
            .iter()
            .any(|w| lower.starts_with(w))
        || value == "?"
        || value == ".";
    if !needs_quote {
        value.to_string()
    } else if !value.contains('\'') {
        format!("'{value}'")
    } else if !value.contains('"') {
        format!("\"{value}\"")
    } else {
        format!("\n;{value}\n;\n")
    }
}

/// `A`, `B`, ..., `Z`, `AA`, `AB`, ...: label_asym_id in order
fn asym_name(mut n: usize) -> String {
    let mut out = vec![];
    loop {
        out.push((b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out.iter().rev().collect()
}

/// A loop of rows, columns padded only by a space
fn write_loop(out: &mut String, category: &str, fields: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    out.push_str("loop_\n");
    for f in fields {
        let _ = writeln!(out, "_{category}.{f}");
    }
    for row in rows {
        out.push_str(&row.join(" "));
        out.push('\n');
    }
    out.push_str("#\n");
}

/// A category with one row, as `_category.field value` lines
fn write_pairs(out: &mut String, category: &str, pairs: &[(&str, String)]) {
    for (f, v) in pairs {
        let _ = writeln!(out, "_{category}.{f} {v}");
    }
    out.push_str("#\n");
}

const STANDARD_AMINO: &[&str] = &[
    "ALA", "ARG", "ASN", "ASP", "CYS", "GLN", "GLU", "GLY", "HIS", "ILE", "LEU", "LYS",
    "MET", "PHE", "PRO", "SER", "THR", "TRP", "TYR", "VAL",
];
const STANDARD_DNA: &[&str] = &["DA", "DC", "DG", "DT"];
const STANDARD_RNA: &[&str] = &["A", "C", "G", "U"];

fn standard(code: &str, ptype: PolymerType) -> bool {
    match ptype {
        PolymerType::Protein => STANDARD_AMINO.contains(&code),
        PolymerType::Dna => STANDARD_DNA.contains(&code),
        PolymerType::Rna => STANDARD_RNA.contains(&code),
    }
}

// --------------------------------------------------
/// What each residue becomes in the file
#[derive(Debug, Clone)]
struct ResidueSite {
    entity: usize,
    asym: String,
    /// label_seq_id: 1, 2, ... within a polymer chain; None otherwise
    seq: Option<usize>,
    /// label_comp_id: the chain table's code in a polymer, else the name
    comp: String,
}

enum EntityKind {
    Polymer {
        ptype: PolymerType,
        /// caps included
        monomers: Vec<String>,
        sequence: String,
        chains: Vec<usize>,
    },
    NonPolymer {
        comp: String,
    },
    Water,
}

// --------------------------------------------------
/// The mmCIF text for one pdb's text
pub fn mmcif_text(
    pdb: &str,
    input: &MmcifInput,
    bonds: &[(usize, usize)],
) -> Result<(String, MmcifCounts)> {
    let atoms = read_atoms(pdb);
    if atoms.is_empty() {
        bail!("no atoms");
    }
    let (chains, residues) = sequence::residue_layout(pdb)?;
    let num_res_atoms: usize = residues.iter().map(|r| r.num_atoms).sum();
    if num_res_atoms != atoms.len() {
        bail!("{} atoms but {num_res_atoms} in residues", atoms.len());
    }

    // ---- Entities, asym ids, label_seq_ids, per residue
    let mut entities: Vec<EntityKind> = vec![];
    let mut polymer_entity: HashMap<(String, Vec<String>), usize> = HashMap::new();
    let mut nonpoly_entity: HashMap<String, usize> = HashMap::new();
    let mut residues_by_comp: BTreeMap<String, usize> = BTreeMap::new();
    let mut water_entity: Option<usize> = None;
    let mut chain_monomers: Vec<Vec<String>> = vec![vec![]; chains.len()];
    for res in &residues {
        if let Some(c) = res.chain {
            chain_monomers[c].push(String::new());
        }
    }
    // The monomers of each chain, caps included, in file order
    for (c, chain) in chains.iter().enumerate() {
        let mut mons = vec![];
        if let Some(cap) = &chain.n_terminal_cap {
            mons.push(cap.clone());
        }
        mons.extend(chain.residues.iter().cloned());
        if let Some(cap) = &chain.c_terminal_cap {
            mons.push(cap.clone());
        }
        if mons.len() != chain_monomers[c].len() {
            bail!(
                "chain {} has {} residues in the file, {} in the chain table",
                c + 1,
                chain_monomers[c].len(),
                mons.len()
            );
        }
        chain_monomers[c] = mons;
    }
    let mut chain_entity = vec![0usize; chains.len()];
    for (c, chain) in chains.iter().enumerate() {
        let key = (chain.polymer_type.to_string(), chain_monomers[c].clone());
        let e = *polymer_entity.entry(key).or_insert_with(|| {
            entities.push(EntityKind::Polymer {
                ptype: chain.polymer_type,
                monomers: chain_monomers[c].clone(),
                sequence: chain.sequence.clone(),
                chains: vec![],
            });
            entities.len() - 1
        });
        if let EntityKind::Polymer { chains: cs, .. } = &mut entities[e] {
            cs.push(c);
        }
        chain_entity[c] = e;
    }

    let mut next_asym = 0usize;
    let mut new_asym = || {
        let a = asym_name(next_asym);
        next_asym += 1;
        a
    };
    let chain_asym: Vec<String> = (0..chains.len()).map(|_| new_asym()).collect();
    let mut water_asym: Option<String> = None;
    let mut ion_asym: HashMap<String, String> = HashMap::new();
    let mut seq_in_chain = vec![0usize; chains.len()];
    let mut sites: Vec<ResidueSite> = vec![];
    let mut asym_entity: BTreeMap<String, usize> = BTreeMap::new();
    for (c, a) in chain_asym.iter().enumerate() {
        asym_entity.insert(a.clone(), chain_entity[c]);
    }
    for res in &residues {
        let first = &atoms[res.first_atom];
        let site = if let Some(c) = res.chain {
            seq_in_chain[c] += 1;
            let n = seq_in_chain[c];
            ResidueSite {
                entity: chain_entity[c],
                asym: chain_asym[c].clone(),
                seq: Some(n),
                comp: chain_monomers[c][n - 1].clone(),
            }
        } else if res.water {
            let e = *water_entity.get_or_insert_with(|| {
                entities.push(EntityKind::Water);
                entities.len() - 1
            });
            let a = water_asym.get_or_insert_with(&mut new_asym).clone();
            asym_entity.insert(a.clone(), e);
            ResidueSite {
                entity: e,
                asym: a,
                seq: None,
                comp: "HOH".to_string(),
            }
        } else {
            let comp = first.resname.clone();
            *residues_by_comp.entry(comp.clone()).or_default() += 1;
            let e = *nonpoly_entity.entry(comp.clone()).or_insert_with(|| {
                entities.push(EntityKind::NonPolymer { comp: comp.clone() });
                entities.len() - 1
            });
            let a = if res.ion {
                ion_asym
                    .entry(comp.clone())
                    .or_insert_with(&mut new_asym)
                    .clone()
            } else {
                new_asym()
            };
            asym_entity.insert(a.clone(), e);
            ResidueSite {
                entity: e,
                asym: a,
                seq: None,
                comp,
            }
        };
        sites.push(site);
    }
    // Residue of each atom
    let mut atom_res = vec![0usize; atoms.len()];
    for (r, res) in residues.iter().enumerate() {
        for slot in &mut atom_res[res.first_atom..res.first_atom + res.num_atoms] {
            *slot = r;
        }
    }

    // auth_asym_id: the pdb's chain ID. Blank on a chain (a file left as
    // written): the chain's label. Blank outside every chain: WAT, ION or
    // HET, the psf's segments for the same atoms (Ken, 2026-11-03), so a
    // membrane is not hundreds of chains
    let auth_asym = |r: usize| -> String {
        let a = &atoms[residues[r].first_atom];
        if !a.chain.is_empty() {
            a.chain.clone()
        } else if residues[r].chain.is_some() {
            sites[r].asym.clone()
        } else if residues[r].water {
            "WAT".to_string()
        } else if residues[r].ion {
            "ION".to_string()
        } else {
            "HET".to_string()
        }
    };
    let ligand_of: HashMap<&str, &LigandInfo> = input
        .ligands
        .iter()
        .map(|l| (l.resname.as_str(), l))
        .collect();

    // ---- Write
    let mut out = String::new();
    let entry = if input.entry_id.is_empty() {
        "MDREPO"
    } else {
        input.entry_id.as_str()
    };
    let _ = writeln!(out, "data_{}", entry.replace(char::is_whitespace, "_"));
    out.push_str("#\n");
    write_pairs(&mut out, "entry", &[("id", cif(entry))]);
    write_pairs(
        &mut out,
        "struct",
        &[("entry_id", cif(entry)), ("title", cif(&input.title))],
    );
    // GROMACS writes CRYST1 1.000 1.000 1.000 for no box: not a box
    // (`sequence::MIN_CELL_EDGE`'s reasoning), so no _cell
    if let Some((len, ang)) =
        read_cell(pdb).filter(|(len, _)| len.iter().all(|&l| l >= 10.0))
    {
        write_pairs(
            &mut out,
            "cell",
            &[
                ("entry_id", cif(entry)),
                ("length_a", format!("{:.3}", len[0])),
                ("length_b", format!("{:.3}", len[1])),
                ("length_c", format!("{:.3}", len[2])),
                ("angle_alpha", format!("{:.2}", ang[0])),
                ("angle_beta", format!("{:.2}", ang[1])),
                ("angle_gamma", format!("{:.2}", ang[2])),
            ],
        );
        write_pairs(
            &mut out,
            "symmetry",
            &[
                ("entry_id", cif(entry)),
                ("space_group_name_H-M", cif("P 1")),
            ],
        );
    }

    // _entity
    let mut molecules = vec![0usize; entities.len()];
    for e in asym_entity.values() {
        molecules[*e] += 1;
    }
    let rows: Vec<Vec<String>> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (kind, desc) = match e {
                EntityKind::Polymer { .. } => ("polymer", "?".to_string()),
                EntityKind::NonPolymer { comp } => (
                    "non-polymer",
                    ligand_of
                        .get(comp.as_str())
                        .map(|l| cif(&l.name))
                        .unwrap_or_else(|| cif(comp)),
                ),
                EntityKind::Water => ("water", "water".to_string()),
            };
            vec![
                (i + 1).to_string(),
                kind.to_string(),
                desc,
                molecules[i].to_string(),
            ]
        })
        .collect();
    write_loop(
        &mut out,
        "entity",
        &["id", "type", "pdbx_description", "pdbx_number_of_molecules"],
        &rows,
    );

    // _entity_poly, _entity_poly_seq
    let mut poly_rows = vec![];
    let mut seq_rows = vec![];
    for (i, e) in entities.iter().enumerate() {
        if let EntityKind::Polymer {
            ptype,
            monomers,
            sequence,
            chains: cs,
        } = e
        {
            let kind = match ptype {
                PolymerType::Protein => "polypeptide(L)",
                PolymerType::Dna => "polydeoxyribonucleotide",
                PolymerType::Rna => "polyribonucleotide",
            };
            let nonstd = monomers.iter().any(|m| !standard(m, *ptype));
            // One letter per residue (the chain table's); a cap or a
            // non-standard residue as (CODE), as RCSB writes it
            let chain0 = &chains[cs[0]];
            let mut code = String::new();
            let mut can = String::new();
            if let Some(cap) = &chain0.n_terminal_cap {
                let _ = write!(code, "({cap})");
            }
            for (res, letter) in chain0.residues.iter().zip(sequence.chars()) {
                if standard(res, *ptype) {
                    code.push(letter);
                } else {
                    let _ = write!(code, "({res})");
                }
                can.push(letter);
            }
            if let Some(cap) = &chain0.c_terminal_cap {
                let _ = write!(code, "({cap})");
            }
            let strands: Vec<String> = cs
                .iter()
                .map(|&c| {
                    let r = residues
                        .iter()
                        .position(|x| x.chain == Some(c))
                        .unwrap_or(0);
                    auth_asym(r)
                })
                .collect();
            poly_rows.push(vec![
                (i + 1).to_string(),
                cif(kind),
                if nonstd { "yes" } else { "no" }.to_string(),
                cif(&code),
                cif(&can),
                cif(&strands.join(",")),
            ]);
            for (n, m) in monomers.iter().enumerate() {
                seq_rows.push(vec![
                    (i + 1).to_string(),
                    (n + 1).to_string(),
                    cif(m),
                    "n".into(),
                ]);
            }
        }
    }
    write_loop(
        &mut out,
        "entity_poly",
        &[
            "entity_id",
            "type",
            "nstd_monomer",
            "pdbx_seq_one_letter_code",
            "pdbx_seq_one_letter_code_can",
            "pdbx_strand_id",
        ],
        &poly_rows,
    );
    write_loop(
        &mut out,
        "entity_poly_seq",
        &["entity_id", "num", "mon_id", "hetero"],
        &seq_rows,
    );

    // _pdbx_entity_nonpoly
    let rows: Vec<Vec<String>> = entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            EntityKind::NonPolymer { comp } => Some(vec![
                (i + 1).to_string(),
                ligand_of
                    .get(comp.as_str())
                    .map(|l| cif(&l.name))
                    .unwrap_or_else(|| cif(comp)),
                cif(comp),
            ]),
            EntityKind::Water => {
                Some(vec![(i + 1).to_string(), "water".into(), "HOH".into()])
            }
            _ => None,
        })
        .collect();
    write_loop(
        &mut out,
        "pdbx_entity_nonpoly",
        &["entity_id", "name", "comp_id"],
        &rows,
    );

    // _struct_asym
    let rows: Vec<Vec<String>> = asym_entity
        .iter()
        .map(|(a, e)| vec![cif(a), "N".into(), (e + 1).to_string()])
        .collect();
    let mut rows = rows;
    rows.sort_by_key(|r| (r[0].len(), r[0].clone()));
    write_loop(
        &mut out,
        "struct_asym",
        &["id", "pdbx_blank_PDB_chainid_flag", "entity_id"],
        &rows,
    );

    // _chem_comp: every label_comp_id
    let mut comps: BTreeMap<String, String> = BTreeMap::new();
    for (r, site) in sites.iter().enumerate() {
        let kind = match residues[r].chain.map(|c| chains[c].polymer_type) {
            Some(PolymerType::Protein) => "'L-peptide linking'",
            Some(PolymerType::Dna) => "'DNA linking'",
            Some(PolymerType::Rna) => "'RNA linking'",
            None => "non-polymer",
        };
        comps
            .entry(site.comp.clone())
            .or_insert_with(|| kind.to_string());
    }
    let rows: Vec<Vec<String>> = comps
        .iter()
        .map(|(c, kind)| {
            let name = ligand_of
                .get(c.as_str())
                .map(|l| cif(&l.name))
                .unwrap_or_else(|| "?".into());
            vec![cif(c), kind.clone(), name]
        })
        .collect();
    write_loop(&mut out, "chem_comp", &["id", "type", "name"], &rows);

    // _pdbx_chem_comp_descriptor: the ligands' SMILES and InChIKey
    let mut rows = vec![];
    for l in &input.ligands {
        if !comps.contains_key(&l.resname) {
            continue;
        }
        if let Some(s) = &l.smiles {
            rows.push(vec![
                cif(&l.resname),
                "SMILES".into(),
                "MDRepo".into(),
                cif(s),
            ]);
        }
        if let Some(k) = &l.inchikey {
            rows.push(vec![
                cif(&l.resname),
                "InChIKey".into(),
                "MDRepo".into(),
                cif(k),
            ]);
        }
    }
    write_loop(
        &mut out,
        "pdbx_chem_comp_descriptor",
        &["comp_id", "type", "program", "descriptor"],
        &rows,
    );

    // _struct_ref, _struct_ref_seq: per entity from its first chain, per chain
    let mut ref_rows = vec![];
    let mut seq_rows = vec![];
    let mut ref_id: HashMap<(usize, String), usize> = HashMap::new();
    for (c, refs) in input.references.iter().enumerate().take(chains.len()) {
        let n_cap = usize::from(chains[c].n_terminal_cap.is_some());
        let r0 = residues
            .iter()
            .position(|x| x.chain == Some(c))
            .unwrap_or(0);
        let chain_res: Vec<usize> = (0..residues.len())
            .filter(|&r| residues[r].chain == Some(c))
            .collect();
        for u in refs {
            let key = (chain_entity[c], u.accession.clone());
            let id = *ref_id.entry(key).or_insert_with(|| {
                ref_rows.push(vec![
                    (ref_rows.len() + 1).to_string(),
                    "UNP".into(),
                    cif(&u.accession),
                    cif(&u.accession),
                    (chain_entity[c] + 1).to_string(),
                ]);
                ref_rows.len()
            });
            let beg = u.query_start.max(1) as usize + n_cap;
            let end = u.query_end.max(1) as usize + n_cap;
            let auth = |seq: usize| {
                chain_res
                    .get(seq - 1)
                    .map(|&r| atoms[residues[r].first_atom].resseq.clone())
                    .unwrap_or_default()
            };
            seq_rows.push(vec![
                (seq_rows.len() + 1).to_string(),
                id.to_string(),
                cif(&auth_asym(r0)),
                beg.to_string(),
                end.to_string(),
                u.reference_start.to_string(),
                u.reference_end.to_string(),
                cif(&auth(beg)),
                cif(&auth(end)),
            ]);
        }
    }
    write_loop(
        &mut out,
        "struct_ref",
        &["id", "db_name", "db_code", "pdbx_db_accession", "entity_id"],
        &ref_rows,
    );
    write_loop(
        &mut out,
        "struct_ref_seq",
        &[
            "align_id",
            "ref_id",
            "pdbx_strand_id",
            "seq_align_beg",
            "seq_align_end",
            "db_align_beg",
            "db_align_end",
            "pdbx_auth_seq_align_beg",
            "pdbx_auth_seq_align_end",
        ],
        &seq_rows,
    );

    // _struct_conn: bonds between residues, not a chain's own backbone
    let backbone = |a: usize, b: usize| -> bool {
        let (ra, rb) = (atom_res[a], atom_res[b]);
        let (Some(ca), Some(cb)) = (residues[ra].chain, residues[rb].chain) else {
            return false;
        };
        if ca != cb || ra.abs_diff(rb) != 1 {
            return false;
        }
        let names = [atoms[a].name.as_str(), atoms[b].name.as_str()];
        matches!(names, ["C", "N"] | ["N", "C"] | ["O3'", "P"] | ["P", "O3'"] | ["O3*", "P"] | ["P", "O3*"])
            // a cap's own link to its residue
            || chains[ca].n_terminal_cap.is_some() || chains[ca].c_terminal_cap.is_some()
    };
    let mut conn_rows = vec![];
    let mut kinds: Vec<&str> = vec![];
    for &(i, j) in bonds {
        if i >= atoms.len() || j >= atoms.len() || atom_res[i] == atom_res[j] {
            continue;
        }
        let (ri, rj) = (atom_res[i], atom_res[j]);
        if residues[ri].water || residues[rj].water || backbone(i, j) {
            continue;
        }
        let kind = if atoms[i].name == "SG" && atoms[j].name == "SG" {
            "disulf"
        } else {
            "covale"
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        let ptnr = |a: usize, r: usize| -> Vec<String> {
            vec![
                cif(&sites[r].asym),
                cif(&sites[r].comp),
                sites[r]
                    .seq
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| ".".into()),
                cif(&atoms[a].name),
                cif(&auth_asym(r)),
                cif(&atoms[a].resseq),
            ]
        };
        let mut row = vec![format!("{kind}{}", conn_rows.len() + 1), kind.to_string()];
        row.extend(ptnr(i, ri));
        row.extend(ptnr(j, rj));
        conn_rows.push(row);
    }
    write_loop(
        &mut out,
        "struct_conn",
        &[
            "id",
            "conn_type_id",
            "ptnr1_label_asym_id",
            "ptnr1_label_comp_id",
            "ptnr1_label_seq_id",
            "ptnr1_label_atom_id",
            "ptnr1_auth_asym_id",
            "ptnr1_auth_seq_id",
            "ptnr2_label_asym_id",
            "ptnr2_label_comp_id",
            "ptnr2_label_seq_id",
            "ptnr2_label_atom_id",
            "ptnr2_auth_asym_id",
            "ptnr2_auth_seq_id",
        ],
        &conn_rows,
    );
    let rows: Vec<Vec<String>> = kinds.iter().map(|k| vec![k.to_string()]).collect();
    write_loop(&mut out, "struct_conn_type", &["id"], &rows);

    // _atom_site
    out.push_str("loop_\n");
    for f in [
        "group_PDB",
        "id",
        "type_symbol",
        "label_atom_id",
        "label_alt_id",
        "label_comp_id",
        "label_asym_id",
        "label_entity_id",
        "label_seq_id",
        "pdbx_PDB_ins_code",
        "Cartn_x",
        "Cartn_y",
        "Cartn_z",
        "occupancy",
        "B_iso_or_equiv",
        "pdbx_formal_charge",
        "auth_seq_id",
        "auth_comp_id",
        "auth_asym_id",
        "auth_atom_id",
        "pdbx_PDB_model_num",
    ] {
        let _ = writeln!(out, "_atom_site.{f}");
    }
    for (k, a) in atoms.iter().enumerate() {
        let r = atom_res[k];
        let s = &sites[r];
        let _ = writeln!(
            out,
            "{} {} {} {} . {} {} {} {} {} {:.3} {:.3} {:.3} {:.2} {:.2} {} {} {} {} {} 1",
            if a.hetatm { "HETATM" } else { "ATOM" },
            k + 1,
            cif(if a.element.is_empty() {
                "X"
            } else {
                &a.element
            }),
            cif(&a.name),
            cif(&s.comp),
            cif(&s.asym),
            s.entity + 1,
            s.seq.map(|n| n.to_string()).unwrap_or_else(|| ".".into()),
            if a.icode.is_empty() {
                "?".to_string()
            } else {
                cif(&a.icode)
            },
            a.xyz[0],
            a.xyz[1],
            a.xyz[2],
            a.occupancy,
            a.bfactor,
            a.charge
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into()),
            cif(&a.resseq),
            cif(&a.resname),
            cif(&auth_asym(r)),
            cif(&a.name),
        );
    }
    out.push_str("#\n");

    let counts = MmcifCounts {
        num_atoms: atoms.len(),
        num_entities: entities.len(),
        num_chains: asym_entity.len(),
        num_links: conn_rows.len(),
        residues_by_comp,
    };
    Ok((out, counts))
}

// --------------------------------------------------
/// Read an mmCIF this module wrote back: each atom's name, residue name
/// (auth) and coordinates, from `_atom_site`
fn read_back(text: &str) -> Result<Vec<(String, String, [f64; 3])>> {
    let mut fields: Vec<&str> = vec![];
    let mut in_loop = false;
    let mut out = vec![];
    for line in text.lines() {
        if line.starts_with("_atom_site.") {
            in_loop = true;
            fields.push(&line["_atom_site.".len()..]);
            continue;
        }
        if !in_loop || fields.is_empty() {
            continue;
        }
        if line.starts_with('#') || line.starts_with("loop_") || line.starts_with('_') {
            break;
        }
        let vals = split_cif(line);
        if vals.len() != fields.len() {
            bail!(
                "_atom_site row with {} values, {} fields: {line}",
                vals.len(),
                fields.len()
            );
        }
        let get = |f: &str| -> Result<&str> {
            let i = fields
                .iter()
                .position(|x| *x == f)
                .ok_or_else(|| anyhow!("no {f}"))?;
            Ok(vals[i].as_str())
        };
        let xyz = [
            get("Cartn_x")?.parse::<f64>()?,
            get("Cartn_y")?.parse::<f64>()?,
            get("Cartn_z")?.parse::<f64>()?,
        ];
        out.push((
            get("auth_atom_id")?.to_string(),
            get("auth_comp_id")?.to_string(),
            xyz,
        ));
    }
    Ok(out)
}

/// Split one CIF data line into values, honouring '...' and "..." quotes
fn split_cif(line: &str) -> Vec<String> {
    let mut vals = vec![];
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '\'' || chars[i] == '"' {
            let q = chars[i];
            let mut j = i + 1;
            // a closing quote is one followed by whitespace or the end
            while j < chars.len()
                && !(chars[j] == q
                    && (j + 1 == chars.len() || chars[j + 1].is_whitespace()))
            {
                j += 1;
            }
            vals.push(chars[i + 1..j.min(chars.len())].iter().collect());
            i = j + 1;
        } else {
            let mut j = i;
            while j < chars.len() && !chars[j].is_whitespace() {
                j += 1;
            }
            vals.push(chars[i..j].iter().collect());
            i = j;
        }
    }
    vals
}

/// Write `path` from `pdb`'s text and check it reads back as the pdb's
/// atoms; a file that does not is removed
fn write_checked(
    path: &Path,
    pdb: &str,
    input: &MmcifInput,
    bonds: &[(usize, usize)],
) -> Result<MmcifCounts> {
    let (text, counts) = mmcif_text(pdb, input, bonds)?;
    let atoms = read_atoms(pdb);
    fs::write(path, &text)?;
    let back = read_back(&fs::read_to_string(path)?);
    let ok = match &back {
        Ok(back) => {
            back.len() == atoms.len()
                && back.iter().zip(&atoms).all(|((n, r, xyz), a)| {
                    *n == a.name
                        && *r == a.resname
                        && (0..3).all(|k| (xyz[k] - a.xyz[k]).abs() < 0.0006)
                })
        }
        Err(_) => false,
    };
    if !ok {
        let _ = fs::remove_file(path);
        bail!(
            "{} did not read back as the pdb's {} atoms ({})",
            path.display(),
            atoms.len(),
            back.map(|b| format!("{} read", b.len()))
                .unwrap_or_else(|e| e.to_string())
        );
    }
    Ok(counts)
}

// --------------------------------------------------
/// Write `full_mmcif` and `min_mmcif` from the two pdbs. `bonds_tsv` is
/// extract_bonds.py's full.pdb bonds (two 1-based serials per line), as for
/// the psf; None writes no `_struct_conn`. A minimal atom's bonds are its
/// full.pdb atom's (`psf_writer::find_in_full`).
pub fn write_mmcifs(
    full_pdb: &Path,
    min_pdb: &Path,
    bonds_tsv: Option<&Path>,
    full_mmcif: &Path,
    min_mmcif: &Path,
    input: &MmcifInput,
) -> Result<(MmcifCounts, MmcifCounts)> {
    let full_text = fs::read_to_string(full_pdb)?;
    let min_text = fs::read_to_string(min_pdb)?;
    let full_atoms = psf_writer::read_pdb(&full_text);
    let bonds = match bonds_tsv {
        Some(p) => psf_writer::read_bonds(&fs::read_to_string(p)?, full_atoms.len())?,
        None => vec![],
    };
    let min_bonds = if bonds.is_empty() {
        vec![]
    } else {
        let min_atoms = psf_writer::read_pdb(&min_text);
        let in_full = psf_writer::find_in_full(&min_atoms, &full_atoms)
            .map_err(|e| anyhow!("{}: {e}", min_pdb.display()))?;
        let to_min: HashMap<usize, usize> =
            in_full.iter().enumerate().map(|(m, &f)| (f, m)).collect();
        bonds
            .iter()
            .filter_map(|&(i, j)| Some((*to_min.get(&i)?, *to_min.get(&j)?)))
            .collect()
    };
    let full = write_checked(full_mmcif, &full_text, input, &bonds)?;
    let min = match write_checked(min_mmcif, &min_text, input, &min_bonds) {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(full_mmcif);
            return Err(e);
        }
    };
    Ok((full, min))
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn atom(
        serial: usize,
        name: &str,
        res: &str,
        chain: char,
        num: i32,
        el: &str,
        x: f64,
    ) -> String {
        format!(
            "ATOM  {serial:>5} {name:<4} {res:>3} {chain}{num:>4}    {x:>8.3}{:>8.3}{:>8.3}  1.00  0.00          {el:>2}\n",
            0.0, 0.0
        )
    }

    /// Two identical chains (a homodimer, blank IDs) with a Cys each,
    /// a ligand, and two waters both numbered 1
    fn dimer() -> String {
        let mut s =
            "CRYST1   50.000   50.000   50.000  90.00  90.00  90.00 P 1           1\n"
                .to_string();
        let mut n = 0;
        for (c, x0) in [(' ', 0.0), (' ', 100.0)] {
            for (i, res) in ["ALA", "CYS", "GLY"].iter().enumerate() {
                for (name, dx) in [("N", 0.0), ("CA", 1.2), ("C", 2.47)] {
                    n += 1;
                    s += &atom(
                        n,
                        name,
                        res,
                        c,
                        i as i32 + 1,
                        &name[..1],
                        x0 + 3.8 * i as f64 + dx,
                    );
                }
                if *res == "CYS" {
                    n += 1;
                    s += &atom(
                        n,
                        "SG",
                        res,
                        c,
                        i as i32 + 1,
                        "S",
                        x0 + 3.8 * i as f64 + 0.5,
                    );
                }
            }
            s += "TER\n";
        }
        n += 1;
        s += &atom(n, "C1", "LIG", ' ', 9, "C", 300.0);
        n += 1;
        s += &atom(n, "O1", "LIG", ' ', 9, "O", 301.2);
        for x in [400.0, 410.0] {
            n += 1;
            s += &atom(n, "O", "HOH", ' ', 1, "O", x);
        }
        s + "END\n"
    }

    #[test]
    fn cif_values_are_quoted_when_they_must_be() {
        assert_eq!(cif("CA"), "CA");
        assert_eq!(cif(""), "?");
        assert_eq!(cif("O5'"), "\"O5'\"");
        assert_eq!(cif("P 1"), "'P 1'");
        assert_eq!(cif("_x"), "'_x'");
        assert_eq!(cif("data_x"), "'data_x'");
        assert_eq!(split_cif("A 'P 1' \"O5'\" x"), ["A", "P 1", "O5'", "x"]);
    }

    #[test]
    fn asym_names_run_on_after_z() {
        assert_eq!(asym_name(0), "A");
        assert_eq!(asym_name(25), "Z");
        assert_eq!(asym_name(26), "AA");
        assert_eq!(asym_name(27), "AB");
        assert_eq!(asym_name(26 + 26 * 26), "AAA");
    }

    #[test]
    fn a_homodimer_is_one_entity_and_links_skip_the_backbone() {
        let pdb = dimer();
        let input = MmcifInput {
            entry_id: "MDR00000001".into(),
            ligands: vec![LigandInfo {
                resname: "LIG".into(),
                name: "a ligand".into(),
                smiles: Some("CO".into()),
                inchikey: Some("OKKJLVBELUTLKV-UHFFFAOYSA-N".into()),
            }],
            references: vec![vec![UniprotRef {
                accession: "P12345".into(),
                query_start: 1,
                query_end: 3,
                reference_start: 10,
                reference_end: 12,
            }]],
            ..Default::default()
        };
        // atoms: chain 1 = 0..10 (SG at 6), chain 2 = 10..20 (SG at 16),
        // the ligand 20-21
        let bonds = vec![(2, 3), (6, 16), (20, 21)];
        let (text, counts) = mmcif_text(&pdb, &input, &bonds).unwrap();
        // polymer, ligand, water
        assert_eq!(counts.num_entities, 3);
        // two chains, the ligand, the water
        assert_eq!(counts.num_chains, 4);
        // only the disulfide: C-N is backbone, the ligand's bond is inside
        // its residue
        assert_eq!(counts.num_links, 1);
        assert!(
            text.contains("disulf1 disulf A CYS 2 SG A 2 B CYS 2 SG B 2"),
            "{}",
            &text[text.find("_struct_conn.ptnr2_auth_seq_id").unwrap_or(0)..][..200]
        );
        assert!(text.contains("_cell.length_a 50.000"));
        assert!(text.contains("1 polymer ? 2"));
        assert!(text.contains("LIG SMILES MDRepo CO"));
        assert!(text.contains("1 1 A 1 3 10 12 1 3"));
        let back = read_back(&text).unwrap();
        assert_eq!(back.len(), 24);
        assert_eq!(back[6].0, "SG");
        // the ligand and the water, blank in the pdb: HET and WAT
        assert!(
            text.lines()
                .any(|l| l.starts_with("ATOM 21 ") && l.contains(" HET "))
        );
        assert!(
            text.lines()
                .any(|l| l.starts_with("ATOM 23 ") && l.contains(" WAT "))
        );
    }

    #[test]
    fn ligand_copies_are_counted_per_residue_name() {
        // The dimer's one LIG, two more copies of it, and two sodium ions;
        // the polymer and the waters are not counted
        let mut pdb = dimer().replace("END\n", "");
        let mut n = 24;
        for num in [10, 11] {
            for (name, el, dx) in [("C1", "C", 0.0), ("O1", "O", 1.2)] {
                n += 1;
                pdb +=
                    &atom(n, name, "LIG", ' ', num, el, 300.0 + 10.0 * num as f64 + dx);
            }
        }
        for num in [20, 21] {
            n += 1;
            pdb += &atom(n, "NA", "NA", ' ', num, "NA", 500.0 + 10.0 * num as f64);
        }
        pdb += "END\n";

        let (_, counts) = mmcif_text(&pdb, &MmcifInput::default(), &[]).unwrap();
        assert_eq!(
            counts.residues_by_comp,
            BTreeMap::from([("LIG".to_string(), 3), ("NA".to_string(), 2)])
        );
    }

    #[test]
    fn a_placeholder_box_is_no_cell() {
        let pdb = dimer()
            .replace("   50.000   50.000   50.000", "    1.000    1.000    1.000");
        let (text, _) = mmcif_text(&pdb, &MmcifInput::default(), &[]).unwrap();
        assert!(!text.contains("_cell."));
        assert!(text.starts_with("data_MDREPO\n"));
    }

    #[test]
    fn the_pair_is_written_and_read_back() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let full = dir.path().join("full.pdb");
        let min = dir.path().join("minimal.pdb");
        fs::write(&full, dimer())?;
        // minimal: no water
        fs::write(
            &min,
            dimer()
                .lines()
                .filter(|l| !l.contains("HOH"))
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
        let (f, m) = write_mmcifs(
            &full,
            &min,
            None,
            &dir.path().join("full.mmcif"),
            &dir.path().join("minimal.mmcif"),
            &MmcifInput::default(),
        )?;
        assert_eq!((f.num_atoms, m.num_atoms), (24, 22));
        assert!(dir.path().join("minimal.mmcif").is_file());
        Ok(())
    }
}
