//! The standard `.psf`: `full.psf` beside `full.pdb`, `minimal.psf` beside
//! `minimal.pdb`.
//!
//! The standard files are pdb, xtc and psf (Travis, 2026-10-22), and the psf
//! is the graph only: atoms, elements, residues, chains (segments) and bonds.
//! It holds no force-field numbers. The format still has charge, mass and
//! type columns, so the charge is 0, the mass is the element's standard
//! weight and the type is the element symbol. Angles, dihedrals and the rest
//! are empty sections.
//!
//! The bonds come from simulation-processing `extract_bonds.py`, which maps
//! the best topology the upload has onto `full.pdb`'s atoms, or guesses them
//! from distances when it has none, and says which (`BondSource`). Each
//! `minimal.pdb` atom is found in `full.pdb` in order, by atom and residue
//! name, as `psf_elements::rewrite` finds a PDB's atoms in a `.psf`: the
//! minimal file drops whole residues, so the first match onward is the right
//! one. A bond is kept in `minimal.psf` when both its atoms are.
//!
//! The segment of an atom is its chain ID. Where the chain column is blank
//! (MDR-106: many files separate chains only with TER), each TER-separated
//! block is a segment, lettered from A.
//!
//! Each file written is read back with `psf_elements::parse_psf` and must
//! hold the atoms and bonds it was given, or it is removed: a half-written
//! file is never left to be published.

use anyhow::{Result, anyhow, bail};
use std::{collections::HashMap, fs, path::Path};

use crate::psf_elements::{WEIGHTS, parse_psf};

/// Bonds longer than this (Å, no periodic image) are counted for a warning
pub const LONG_BOND: f64 = 2.2;

/// One ATOM/HETATM record of the first model
#[derive(Debug, Clone, PartialEq)]
pub struct PdbAtom {
    pub name: String,
    pub resname: String,
    /// Residue number and insertion code, as written
    pub resid: String,
    pub segid: String,
    pub element: String,
    pub xyz: [f64; 3],
}

/// What a written psf holds
#[derive(Debug, PartialEq)]
pub struct PsfCounts {
    pub num_atoms: usize,
    pub num_bonds: usize,
    pub long_bonds: usize,
}

// --------------------------------------------------
/// Write `full_psf` and `min_psf` from the two PDBs and `bonds_tsv` (two
/// 1-based `full.pdb` atom serials per line, as extract_bonds.py writes).
/// `title` is one line for the header, e.g. where the bonds came from.
pub fn write_psfs(
    full_pdb: &Path,
    min_pdb: &Path,
    bonds_tsv: &Path,
    full_psf: &Path,
    min_psf: &Path,
    title: &str,
) -> Result<(PsfCounts, PsfCounts)> {
    let full = read_pdb(&fs::read_to_string(full_pdb)?);
    let min = read_pdb(&fs::read_to_string(min_pdb)?);
    if full.is_empty() {
        bail!("{} has no atoms", full_pdb.display());
    }
    let bonds = read_bonds(&fs::read_to_string(bonds_tsv)?, full.len())?;

    let in_full =
        find_in_full(&min, &full).map_err(|e| anyhow!("{}: {e}", min_pdb.display()))?;
    let to_min: HashMap<usize, usize> =
        in_full.iter().enumerate().map(|(m, &f)| (f, m)).collect();
    let min_bonds: Vec<(usize, usize)> = bonds
        .iter()
        .filter_map(|&(i, j)| Some((*to_min.get(&i)?, *to_min.get(&j)?)))
        .collect();

    let full_counts = write_checked(full_psf, &full, &bonds, title)?;
    let min_counts = match write_checked(min_psf, &min, &min_bonds, title) {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(full_psf);
            return Err(e);
        }
    };
    Ok((full_counts, min_counts))
}

// --------------------------------------------------
fn write_checked(
    path: &Path,
    atoms: &[PdbAtom],
    bonds: &[(usize, usize)],
    title: &str,
) -> Result<PsfCounts> {
    let text = psf_text(atoms, bonds, title);
    fs::write(path, &text)?;
    let back = parse_psf(&text);
    let ok = matches!(&back, Ok(p) if p.names.len() == atoms.len() && p.bonds.len() == bonds.len());
    if !ok {
        let _ = fs::remove_file(path);
        bail!(
            "{} did not read back as written: {}",
            path.display(),
            match back {
                Err(e) => e,
                Ok(p) => format!(
                    "{} atoms and {} bonds, not {} and {}",
                    p.names.len(),
                    p.bonds.len(),
                    atoms.len(),
                    bonds.len()
                ),
            }
        );
    }
    Ok(PsfCounts {
        num_atoms: atoms.len(),
        num_bonds: bonds.len(),
        long_bonds: count_long(atoms, bonds),
    })
}

// --------------------------------------------------
/// The atoms of a PDB's first model, with each atom's segment
pub fn read_pdb(text: &str) -> Vec<PdbAtom> {
    let mut atoms = vec![];
    let mut block = 0usize;
    let mut block_has_atoms = false;
    for line in text.lines() {
        if line.starts_with("ENDMDL") {
            break;
        }
        if line.starts_with("TER") {
            if block_has_atoms {
                block += 1;
                block_has_atoms = false;
            }
            continue;
        }
        if !(line.starts_with("ATOM  ") || line.starts_with("HETATM"))
            || !line.is_ascii()
        {
            continue;
        }
        let col =
            |a: usize, b: usize| line.get(a..b.min(line.len())).unwrap_or("").trim();
        let coord = |a: usize, b: usize| col(a, b).parse::<f64>().unwrap_or(0.0);
        let name = col(12, 16).to_string();
        let chain = col(21, 22);
        let segid = if chain.is_empty() {
            block_letter(block)
        } else {
            chain.to_string()
        };
        atoms.push(PdbAtom {
            element: element_of(col(76, 78), &name),
            name,
            // cpptraj right-aligns the residue name in columns 17-20, so a
            // four-letter one (TIP3) starts in the alternate-location column
            resname: col(16, 21).to_string(),
            resid: col(22, 27).to_string(),
            segid,
            xyz: [coord(30, 38), coord(38, 46), coord(46, 54)],
        });
        block_has_atoms = true;
    }
    atoms
}

// --------------------------------------------------
/// A, B, ..., Z, then S27, S28, ...
fn block_letter(block: usize) -> String {
    if block < 26 {
        ((b'A' + block as u8) as char).to_string()
    } else {
        format!("S{}", block + 1)
    }
}

// --------------------------------------------------
/// The element column, or a guess from the atom name
fn element_of(column: &str, name: &str) -> String {
    let col = column.trim().to_uppercase();
    if !col.is_empty() && col.chars().all(|c| c.is_ascii_alphabetic()) {
        return col;
    }
    let letters = name
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .to_uppercase();
    if letters.starts_with("CL") || letters.starts_with("BR") {
        letters[..2].to_string()
    } else {
        letters.chars().take(1).collect()
    }
}

// --------------------------------------------------
/// Bonds as 0-based pairs, each atom checked against `num_atoms`
pub fn read_bonds(text: &str, num_atoms: usize) -> Result<Vec<(usize, usize)>> {
    let mut bonds = vec![];
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let ids: Vec<usize> = line
            .split_whitespace()
            .map(|w| w.parse::<usize>())
            .collect::<Result<_, _>>()
            .map_err(|_| anyhow!("bonds line {}: {line:?}", n + 1))?;
        match ids[..] {
            [i, j]
                if (1..=num_atoms).contains(&i)
                    && (1..=num_atoms).contains(&j)
                    && i != j =>
            {
                bonds.push((i.min(j) - 1, i.max(j) - 1))
            }
            _ => bail!(
                "bonds line {}: {line:?} is not two of {num_atoms} atoms",
                n + 1
            ),
        }
    }
    bonds.sort_unstable();
    bonds.dedup();
    Ok(bonds)
}

// --------------------------------------------------
/// For each `min` atom, its index in `full`, found in order by atom and
/// residue name
pub fn find_in_full(min: &[PdbAtom], full: &[PdbAtom]) -> Result<Vec<usize>, String> {
    let mut next = 0;
    let mut found = Vec::with_capacity(min.len());
    for (k, m) in min.iter().enumerate() {
        let Some(i) = (next..full.len())
            .find(|&i| full[i].name == m.name && full[i].resname == m.resname)
        else {
            return Err(format!(
                "atom {} ({} {}) is not in full.pdb after atom {next}",
                k + 1,
                m.name,
                m.resname
            ));
        };
        found.push(i);
        next = i + 1;
    }
    Ok(found)
}

// --------------------------------------------------
fn mass_of(element: &str) -> f64 {
    WEIGHTS
        .iter()
        .find(|(e, _)| *e == element)
        .map(|&(_, w)| w)
        .unwrap_or(0.0)
}

// --------------------------------------------------
/// The file, in psfgen's PSF EXT layout
pub fn psf_text(atoms: &[PdbAtom], bonds: &[(usize, usize)], title: &str) -> String {
    let mut s = String::with_capacity(atoms.len() * 100 + bonds.len() * 20 + 1024);
    s.push_str("PSF EXT\n\n");
    s.push_str("         3 !NTITLE\n");
    s.push_str(
        "* MDRepo standard topology: atoms, elements, residues, segments, bonds\n",
    );
    s.push_str(
        "* No force-field parameters: charges 0, masses standard, types = elements\n",
    );
    s.push_str(&format!("* {}\n\n", title.replace('\n', " ")));

    s.push_str(&format!("{:>10} !NATOM\n", atoms.len()));
    for (k, a) in atoms.iter().enumerate() {
        let elem = if a.element.is_empty() {
            "X"
        } else {
            a.element.as_str()
        };
        s.push_str(&format!(
            "{:>10} {:<8} {:<8} {:<8} {:<8} {:<6} {:>10.6}    {:>10.4}  {:>10}\n",
            k + 1,
            trunc(&a.segid, 8),
            trunc(&a.resid, 8),
            trunc(&a.resname, 8),
            trunc(&a.name, 8),
            trunc(elem, 6),
            0.0,
            mass_of(&a.element),
            0
        ));
    }

    s.push_str(&format!("\n{:>10} !NBOND: bonds\n", bonds.len()));
    for chunk in bonds.chunks(4) {
        for &(i, j) in chunk {
            s.push_str(&format!("{:>10}{:>10}", i + 1, j + 1));
        }
        s.push('\n');
    }

    for section in [
        "!NTHETA: angles",
        "!NPHI: dihedrals",
        "!NIMPHI: impropers",
        "!NDON: donors",
        "!NACC: acceptors",
    ] {
        s.push_str(&format!("\n{:>10} {section}\n\n", 0));
    }

    // NNB is followed by one exclusion pointer per atom, all 0 here, as
    // psfgen writes it; some readers count on that block.
    s.push_str(&format!("\n{:>10} !NNB\n\n", 0));
    for chunk in (0..atoms.len()).collect::<Vec<_>>().chunks(8) {
        for _ in chunk {
            s.push_str(&format!("{:>10}", 0));
        }
        s.push('\n');
    }

    s.push_str(&format!("\n{:>10}{:>10} !NGRP NST2\n", 1, 0));
    s.push_str(&format!("{:>10}{:>10}{:>10}\n\n", 0, 0, 0));
    s
}

// --------------------------------------------------
fn trunc(s: &str, n: usize) -> &str {
    s.get(..n).unwrap_or(s)
}

// --------------------------------------------------
fn count_long(atoms: &[PdbAtom], bonds: &[(usize, usize)]) -> usize {
    bonds
        .iter()
        .filter(|&&(i, j)| {
            let (a, b) = (atoms[i].xyz, atoms[j].xyz);
            let d2: f64 = (0..3).map(|k| (a[k] - b[k]).powi(2)).sum();
            d2.sqrt() > LONG_BOND
        })
        .count()
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn atom(
        serial: usize,
        name: &str,
        resname: &str,
        chain: char,
        resid: i32,
        element: &str,
        x: f64,
    ) -> String {
        let field = if name.len() < 4 {
            format!(" {name:<3}")
        } else {
            name.to_string()
        };
        format!(
            "ATOM  {serial:5} {field} {resname:<4}{chain}{resid:4}    {x:8.3}{:8.3}{:8.3}  1.00  0.00          {element:>2}\n",
            0.0, 0.0
        )
    }

    /// Two chains with a blank chain column, separated by TER, then a water
    fn blank_chain_pdb() -> String {
        let mut s = String::new();
        s += &atom(1, "N", "ALA", ' ', 1, "N", 0.0);
        s += &atom(2, "CA", "ALA", ' ', 1, "C", 1.45);
        s += "TER\n";
        s += &atom(3, "N", "GLY", ' ', 2, "N", 10.0);
        s += &atom(4, "CA", "GLY", ' ', 2, "C", 11.45);
        s += "TER\n";
        s += &atom(5, "OH2", "TIP3", ' ', 3, "O", 20.0);
        s += &atom(6, "H1", "TIP3", ' ', 3, "H", 20.96);
        s += "END\n";
        s
    }

    #[test]
    fn blank_chains_become_one_segment_per_ter_block() {
        let atoms = read_pdb(&blank_chain_pdb());
        let segs: Vec<&str> = atoms.iter().map(|a| a.segid.as_str()).collect();
        assert_eq!(segs, ["A", "A", "B", "B", "C", "C"]);
        assert_eq!(atoms[4].resname, "TIP3");
        assert_eq!(atoms[5].element, "H");
    }

    #[test]
    fn a_chain_id_is_the_segment() {
        let pdb = atom(1, "N", "ALA", 'X', 1, "N", 0.0)
            + "TER\n"
            + &atom(2, "N", "ALA", 'X', 2, "N", 3.0);
        let atoms = read_pdb(&pdb);
        assert_eq!(atoms[0].segid, "X");
        assert_eq!(atoms[1].segid, "X");
    }

    #[test]
    fn the_psf_reads_back_with_its_atoms_bonds_and_masses() {
        let atoms = read_pdb(&blank_chain_pdb());
        let bonds = vec![(0, 1), (2, 3), (4, 5)];
        let text = psf_text(&atoms, &bonds, "bonds from test.psf");
        let back = parse_psf(&text).unwrap();
        assert_eq!(back.names, ["N", "CA", "N", "CA", "OH2", "H1"]);
        assert_eq!(back.bonds, bonds);
        assert_eq!(back.charges, [0.0; 6]);
        assert!((back.masses[1] - 12.011).abs() < 1e-6);
        assert!((back.masses[5] - 1.008).abs() < 1e-6);
        assert!(back.residues[0].starts_with("A 1 ALA"));
        assert!(back.residues[4].starts_with("C 3 TIP3"));
        assert!(text.contains("* bonds from test.psf"));
    }

    #[test]
    fn bonds_are_checked_against_the_atoms() {
        assert_eq!(
            read_bonds("1\t2\n2 3\n2\t1\n", 3).unwrap(),
            [(0, 1), (1, 2)]
        );
        assert!(read_bonds("1\t4\n", 3).is_err());
        assert!(read_bonds("1\t1\n", 3).is_err());
        assert!(read_bonds("1 2 3\n", 3).is_err());
        assert!(read_bonds("a b\n", 3).is_err());
    }

    #[test]
    fn minimal_atoms_are_found_in_full_in_order() {
        let full = read_pdb(&blank_chain_pdb());
        let min: Vec<PdbAtom> = full[..4].to_vec();
        assert_eq!(find_in_full(&min, &full).unwrap(), [0, 1, 2, 3]);
        let mut wrong = full[..2].to_vec();
        wrong[1].name = "CB".into();
        assert!(find_in_full(&wrong, &full).is_err());
    }

    #[test]
    fn write_psfs_drops_bonds_to_atoms_minimal_does_not_keep() -> Result<()> {
        let dir = tempdir()?;
        let full_pdb = dir.path().join("full.pdb");
        let min_pdb = dir.path().join("minimal.pdb");
        let bonds = dir.path().join("bonds.tsv");
        fs::write(&full_pdb, blank_chain_pdb())?;
        // minimal: the two chains, water stripped, rotated (other xyz)
        let min_text: String = blank_chain_pdb()
            .lines()
            .filter(|l| !l.contains("TIP3"))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&min_pdb, min_text)?;
        fs::write(&bonds, "1\t2\n3\t4\n5\t6\n")?;
        let (full_psf, min_psf) =
            (dir.path().join("full.psf"), dir.path().join("minimal.psf"));
        let (f, m) = write_psfs(&full_pdb, &min_pdb, &bonds, &full_psf, &min_psf, "t")?;
        assert_eq!((f.num_atoms, f.num_bonds), (6, 3));
        assert_eq!((m.num_atoms, m.num_bonds), (4, 2));
        assert_eq!(
            parse_psf(&fs::read_to_string(&min_psf)?).unwrap().bonds,
            [(0, 1), (2, 3)]
        );
        Ok(())
    }

    #[test]
    fn a_minimal_atom_missing_from_full_writes_nothing() -> Result<()> {
        let dir = tempdir()?;
        let full_pdb = dir.path().join("full.pdb");
        let min_pdb = dir.path().join("minimal.pdb");
        let bonds = dir.path().join("bonds.tsv");
        fs::write(&full_pdb, blank_chain_pdb())?;
        fs::write(&min_pdb, atom(1, "ZZ", "UNK", 'A', 1, "C", 0.0))?;
        fs::write(&bonds, "1\t2\n")?;
        let (full_psf, min_psf) =
            (dir.path().join("full.psf"), dir.path().join("minimal.psf"));
        assert!(
            write_psfs(&full_pdb, &min_pdb, &bonds, &full_psf, &min_psf, "t").is_err()
        );
        assert!(!full_psf.exists() && !min_psf.exists());
        Ok(())
    }

    #[test]
    fn long_bonds_are_counted() {
        let atoms = read_pdb(&blank_chain_pdb());
        // N(0) to N(10): a 10 A "bond"
        assert_eq!(count_long(&atoms, &[(0, 1), (0, 2)]), 1);
    }

    #[test]
    fn segments_past_z_are_numbered() {
        assert_eq!(block_letter(0), "A");
        assert_eq!(block_letter(25), "Z");
        assert_eq!(block_letter(26), "S27");
    }
}
