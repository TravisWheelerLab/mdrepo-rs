//! Element and ion-charge columns for the PDBs cpptraj writes from a CHARMM
//! `.psf`.
//!
//! A `.psf` carries no elements, so cpptraj assigns each atom the element
//! nearest its mass. Under hydrogen mass repartitioning that is wrong: a
//! hydrogen carrying 3.024 amu is nearer helium than hydrogen. Zenodo
//! 17477539, a NAMD release, had its 1,682 repartitioned hydrogens all reach
//! `minimal.pdb` as `HE`, and `mol_id.py` inferred its vorapaxar as
//! C29H33FHe33N2O4. Heavy atoms escape
//! only because cpptraj falls back to the atom name when a mass is nearer no
//! element at all.
//!
//! The `.psf` settles the element instead. A hydrogen is any atom under
//! 4.6 amu. A heavy atom first has back the mass it lent its hydrogens -- each
//! bonded hydrogen's mass over 1.008 -- and then takes the element whose
//! standard weight is within 0.1 of that. The same release's methyl carbons,
//! 5.963 amu beside three hydrogens of 3.024, come back to 12.011.
//!
//! A single-atom residue bonded to nothing, whose `.psf` charge is a whole
//! nonzero number, is an ion, and that charge goes in columns 79-80. cpptraj
//! writes none, so OpenBabel read the same release's Ca2+ as neutral calcium
//! and inferred calcium hydride.
//!
//! This only ever corrects cpptraj's output. An atom whose mass matches no
//! element, or two, or a PDB whose atoms cannot be found in the `.psf` in
//! order, leaves the file exactly as cpptraj wrote it, with a warning.

use anyhow::{Result, anyhow};
use log::{debug, warn};
use std::{collections::HashMap, fs, path::Path};

/// Standard atomic weights of the elements biomolecular simulations hold.
const WEIGHTS: &[(&str, f64)] = &[
    ("H", 1.008),
    ("LI", 6.94),
    ("B", 10.81),
    ("C", 12.011),
    ("N", 14.007),
    ("O", 15.999),
    ("F", 18.998),
    ("NA", 22.990),
    ("MG", 24.305),
    ("AL", 26.982),
    ("SI", 28.085),
    ("P", 30.974),
    ("S", 32.06),
    ("CL", 35.45),
    ("K", 39.098),
    ("CA", 40.078),
    ("V", 50.942),
    ("CR", 51.996),
    ("MN", 54.938),
    ("FE", 55.845),
    ("CO", 58.933),
    ("NI", 58.693),
    ("CU", 63.546),
    ("ZN", 65.38),
    ("GA", 69.723),
    ("AS", 74.922),
    ("SE", 78.971),
    ("BR", 79.904),
    ("RB", 85.468),
    ("SR", 87.62),
    ("MO", 95.95),
    ("RU", 101.07),
    ("RH", 102.91),
    ("AG", 107.87),
    ("CD", 112.41),
    ("I", 126.90),
    ("CS", 132.91),
    ("BA", 137.33),
    ("W", 183.84),
    ("PT", 195.08),
    ("AU", 196.97),
    ("HG", 200.59),
    ("TL", 204.38),
    ("PB", 207.2),
];

/// Force fields round weights differently (16.00, 15.9994, 15.999); the
/// closest two elements above, Ni and Co, are 0.24 apart.
const WEIGHT_TOLERANCE: f64 = 0.1;

/// Below this an atom is a hydrogen, repartitioned up to four times its
/// weight.
const HYDROGEN_MASS: f64 = 4.6;

/// Below this an atom is a massless site or a Drude particle, with no element.
const VIRTUAL_MASS: f64 = 0.9;

const HYDROGEN_WEIGHT: f64 = 1.008;

/// What a `.psf` states of each atom that settles its element.
#[derive(Debug, Default)]
pub struct PsfAtoms {
    pub names: Vec<String>,
    pub resnames: Vec<String>,
    /// segid, resid and resname together, which name one residue.
    pub residues: Vec<String>,
    pub charges: Vec<f64>,
    pub masses: Vec<f64>,
    pub bonds: Vec<(usize, usize)>,
}

/// Each atom's element, blank for a virtual site, and formal charge.
#[derive(Debug, PartialEq)]
pub struct Settled {
    pub element: &'static str,
    pub formal_charge: i32,
}

// --------------------------------------------------
/// Whether a topology is a CHARMM `.psf`.
pub fn is_psf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("psf"))
}

// --------------------------------------------------
/// Correct the element and ion-charge columns of each PDB cpptraj wrote from
/// the `.psf` at `psf`. See the module comment.
pub fn fix_psf_elements(psf: &Path, pdbs: &[&Path]) -> Result<()> {
    let text = fs::read_to_string(psf)?;
    let atoms = parse_psf(&text).map_err(|e| anyhow!("{}: {e}", psf.display()))?;
    let settled = match settle(&atoms) {
        Ok(settled) => settled,
        Err(e) => {
            warn!("Leaving cpptraj's elements for {}: {e}", psf.display());
            return Ok(());
        }
    };

    for pdb in pdbs {
        let text = fs::read_to_string(pdb)?;
        match rewrite(&text, &atoms, &settled) {
            Ok((fixed, changed)) => {
                debug!(
                    r#"Set the elements of "{}" from "{}" ({changed} lines changed)"#,
                    pdb.display(),
                    psf.display()
                );
                if changed > 0 {
                    let tmp = pdb.with_extension("pdb.elements");
                    fs::write(&tmp, fixed)?;
                    fs::rename(&tmp, pdb)?;
                }
            }
            Err(e) => {
                warn!(r#"Leaving cpptraj's elements in "{}": {e}"#, pdb.display())
            }
        }
    }
    Ok(())
}

// --------------------------------------------------
/// Read the atom and bond sections of a `.psf`. Fields are split on
/// whitespace, which serves the standard, EXT and XPLOR layouts alike.
pub fn parse_psf(text: &str) -> Result<PsfAtoms, String> {
    fn count(line: &str, section: &str) -> Option<usize> {
        let mut f = line.split_whitespace();
        let n = f.next()?.parse().ok()?;
        f.next()?.starts_with(section).then_some(n)
    }

    let mut lines = text.lines();
    let natom = lines
        .by_ref()
        .find_map(|l| count(l, "!NATOM"))
        .ok_or("no !NATOM section")?;

    let mut atoms = PsfAtoms::default();
    for i in 0..natom {
        let line = lines.next().ok_or("it ends inside its atoms")?;
        let f: Vec<&str> = line.split_whitespace().collect();
        let (Some(charge), Some(mass)) = (
            f.get(6).and_then(|s| s.parse().ok()),
            f.get(7).and_then(|s| s.parse().ok()),
        ) else {
            return Err(format!("atom {}: no charge and mass in {line:?}", i + 1));
        };
        atoms.residues.push(format!("{} {} {}", f[1], f[2], f[3]));
        atoms.resnames.push(f[3].to_string());
        atoms.names.push(f[4].to_string());
        atoms.charges.push(charge);
        atoms.masses.push(mass);
    }

    let nbond = lines.by_ref().find_map(|l| count(l, "!NBOND")).unwrap_or(0);
    let mut ids = Vec::with_capacity(2 * nbond);
    for line in lines.by_ref() {
        if ids.len() >= 2 * nbond {
            break;
        }
        for w in line.split_whitespace() {
            match w.parse::<usize>() {
                Ok(id) if (1..=natom).contains(&id) => ids.push(id - 1),
                _ => return Err(format!("{w:?} in its bonds is no atom")),
            }
        }
    }
    if ids.len() != 2 * nbond {
        return Err(format!(
            "it states {nbond} bonds and lists {} atoms of them",
            ids.len()
        ));
    }
    atoms.bonds = ids.chunks(2).map(|p| (p[0], p[1])).collect();
    Ok(atoms)
}

// --------------------------------------------------
/// Settle each atom's element and formal charge, or say which atom could not
/// be settled.
pub fn settle(atoms: &PsfAtoms) -> Result<Vec<Settled>, String> {
    let m = &atoms.masses;
    let mut lent = vec![0.0; m.len()];
    let mut bonded = vec![0usize; m.len()];
    for &(i, j) in &atoms.bonds {
        for (a, h) in [(i, j), (j, i)] {
            bonded[a] += 1;
            if m[a] >= HYDROGEN_MASS && m[h] < HYDROGEN_MASS {
                lent[a] += m[h];
                if m[h] >= VIRTUAL_MASS {
                    lent[a] -= HYDROGEN_WEIGHT;
                }
            }
        }
    }

    let mut size: HashMap<&str, usize> = HashMap::new();
    for r in &atoms.residues {
        *size.entry(r.as_str()).or_default() += 1;
    }

    let mut settled = Vec::with_capacity(m.len());
    for i in 0..m.len() {
        let element = if m[i] < VIRTUAL_MASS {
            ""
        } else if m[i] < HYDROGEN_MASS {
            "H"
        } else {
            let mass = m[i] + lent[i];
            let near: Vec<&str> = WEIGHTS
                .iter()
                .filter(|(_, w)| (w - mass).abs() <= WEIGHT_TOLERANCE)
                .map(|(e, _)| *e)
                .collect();
            match near[..] {
                [e] => e,
                _ => {
                    return Err(format!(
                        "atom {} ({} {}) has mass {:.3}{}, which matches {}",
                        i + 1,
                        atoms.names[i],
                        atoms.resnames[i],
                        m[i],
                        if lent[i] != 0.0 {
                            format!(" ({mass:.3} with its hydrogens')")
                        } else {
                            String::new()
                        },
                        if near.is_empty() {
                            "no element".to_string()
                        } else {
                            near.join(" and ")
                        }
                    ));
                }
            }
        };

        let q = atoms.charges[i];
        let formal_charge = if size[atoms.residues[i].as_str()] == 1
            && bonded[i] == 0
            && q.round() != 0.0
            && (q - q.round()).abs() < 0.01
        {
            q.round() as i32
        } else {
            0
        };
        settled.push(Settled {
            element,
            formal_charge,
        });
    }
    Ok(settled)
}

// --------------------------------------------------
/// Rewrite columns 77-80 of each atom line of `pdb` from `settled`, returning
/// the new text and how many lines changed.
///
/// The PDB's atoms are found in the `.psf` in order, by atom and residue name.
/// `full.pdb` holds every atom; `minimal.pdb` drops whole residues by residue
/// name, so a kept atom's residue name never matches a dropped atom's and the
/// first match onward is the right one. A PDB atom not found is an error.
pub fn rewrite(
    pdb: &str,
    atoms: &PsfAtoms,
    settled: &[Settled],
) -> Result<(String, usize), String> {
    fn trunc(s: &str, n: usize) -> &str {
        s.get(..n).unwrap_or(s)
    }

    let mut out = String::with_capacity(pdb.len());
    let mut next = 0;
    let mut changed = 0;
    for chunk in pdb.split_inclusive('\n') {
        let line = chunk.trim_end_matches(['\n', '\r']);
        let eol = &chunk[line.len()..];
        if !(line.starts_with("ATOM  ") || line.starts_with("HETATM"))
            || !line.is_ascii()
        {
            out.push_str(chunk);
            continue;
        }
        let name = line.get(12..16).unwrap_or("").trim();
        // cpptraj right-aligns the residue name in columns 17-20, so a
        // four-letter one (TIP3) starts in the alternate-location column;
        // other writers put it in 18-21.
        let resname = line.get(16..21).unwrap_or("").trim();
        let found = (next..atoms.names.len()).find(|&i| {
            trunc(&atoms.names[i], 4) == name && trunc(&atoms.resnames[i], 4) == resname
        });
        let Some(i) = found else {
            return Err(format!(
                "atom {name} {resname} is not in the topology after atom {next}"
            ));
        };
        next = i + 1;

        let s = &settled[i];
        let charge = match s.formal_charge {
            0 => "  ".to_string(),
            q if q > 0 => format!("{q}+"),
            q => format!("{}-", -q),
        };
        let mut fixed = format!("{line:<76}");
        let rest = fixed.get(80..).unwrap_or("").to_string();
        fixed.truncate(76);
        fixed.push_str(&format!("{:>2}{charge:>2}", s.element));
        fixed.push_str(&rest);
        if fixed != line {
            changed += 1;
        }
        out.push_str(&fixed);
        out.push_str(eol);
    }
    Ok((out, changed))
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// A `.psf` of the atoms, each "segid resid resname name type charge
    /// mass", and the 1-based bonds.
    fn psf(atoms: &[&str], bonds: &[(usize, usize)]) -> String {
        let mut s = format!(
            "PSF EXT\n\n         1 !NTITLE\n REMARKS test\n\n{:>10} !NATOM\n",
            atoms.len()
        );
        for (i, a) in atoms.iter().enumerate() {
            let f: Vec<&str> = a.split_whitespace().collect();
            s.push_str(&format!(
                "{:>10} {:<8} {:<8} {:<8} {:<8} {:<6} {:>14}{:>14}           0\n",
                i + 1,
                f[0],
                f[1],
                f[2],
                f[3],
                f[4],
                f[5],
                f[6]
            ));
        }
        s.push_str(&format!("\n{:>10} !NBOND: bonds\n", bonds.len()));
        for (k, (a, b)) in bonds.iter().enumerate() {
            s.push_str(&format!("{a:>10}{b:>10}"));
            if k % 4 == 3 || k == bonds.len() - 1 {
                s.push('\n');
            }
        }
        s.push_str("\n         0 !NTHETA: angles\n\n");
        s
    }

    /// A PDB as cpptraj writes one: 80 columns, the residue name right-aligned
    /// in columns 17-20, and its own element guess.
    fn pdb(atoms: &[(&str, &str, &str)]) -> String {
        let mut s = String::from("REMARK cpptraj\n");
        for (i, (name, resname, element)) in atoms.iter().enumerate() {
            s.push_str(&format!(
                "ATOM  {:>5} {:<4}{:>4}  {:>4}       0.000   0.000   0.000  1.00  0.00          {:>2}  \n",
                i + 1,
                name,
                resname,
                1,
                element
            ));
        }
        s.push_str("END\n");
        s
    }

    fn columns(pdb: &str) -> Vec<String> {
        pdb.lines()
            .filter(|l| l.starts_with("ATOM"))
            .map(|l| {
                assert_eq!(l.len(), 80, "{l:?}");
                l[76..80].to_string()
            })
            .collect()
    }

    fn hmr_system() -> PsfAtoms {
        parse_psf(&psf(
            &[
                "LIG 1 LIG C1 CG331 -0.27 5.963",
                "LIG 1 LIG H11 HGA3 0.09 3.024",
                "LIG 1 LIG H12 HGA3 0.09 3.024",
                "LIG 1 LIG H13 HGA3 0.09 3.024",
                "PRO 2 GLN NE2 NH2 -0.62 9.975",
                "PRO 2 GLN HE21 H 0.32 3.024",
                "PRO 2 GLN HE22 H 0.30 3.024",
                "PRO 3 ALA CA CT1 0.07 12.011",
                "WAT 4 TIP3 OH2 OT -0.834 15.9994",
                "WAT 4 TIP3 H1 HT 0.417 1.008",
                "WAT 4 TIP3 H2 HT 0.417 1.008",
                "ION 5 CAL CAL CAL 2.00 40.08",
                "ION 6 CLA CLA CLA -1.00 35.45",
                "LIG 7 LP LP1 LP 0.00 0.000",
            ],
            &[
                (1, 2),
                (1, 3),
                (1, 4),
                (5, 6),
                (5, 7),
                (9, 10),
                (9, 11),
                (10, 11),
                (1, 14),
            ],
        ))
        .unwrap()
    }

    // cpptraj's guesses from the masses, as the NAMD release got them.
    const CPPTRAJ: [(&str, &str, &str); 14] = [
        ("C1", "LIG", "C"),
        ("H11", "LIG", "HE"),
        ("H12", "LIG", "HE"),
        ("H13", "LIG", "HE"),
        ("NE2", "GLN", "N"),
        ("HE21", "GLN", "HE"),
        ("HE22", "GLN", "HE"),
        ("CA", "ALA", "C"),
        ("OH2", "TIP3", "O"),
        ("H1", "TIP3", "H"),
        ("H2", "TIP3", "H"),
        ("CAL", "CAL", "CA"),
        ("CLA", "CLA", "CL"),
        ("LP1", "LP", ""),
    ];

    #[test]
    fn repartitioned_hydrogens_are_hydrogen_and_ions_are_charged() {
        let atoms = hmr_system();
        let settled = settle(&atoms).unwrap();
        let (fixed, changed) = rewrite(&pdb(&CPPTRAJ), &atoms, &settled).unwrap();
        assert_eq!(
            columns(&fixed),
            [
                " C  ", " H  ", " H  ", " H  ", " N  ", " H  ", " H  ", " C  ", " O  ",
                " H  ", " H  ", "CA2+", "CL1-", "    "
            ]
        );
        // Five hydrogens and two ions; every other line is as cpptraj wrote it.
        assert_eq!(changed, 7);
    }

    #[test]
    fn a_minimal_pdb_is_found_in_the_topology_past_the_stripped_residues() {
        let atoms = hmr_system();
        let settled = settle(&atoms).unwrap();
        let kept: Vec<_> = CPPTRAJ
            .iter()
            .copied()
            .filter(|(_, r, _)| !["TIP3", "CLA"].contains(r))
            .collect();
        let (fixed, _) = rewrite(&pdb(&kept), &atoms, &settled).unwrap();
        assert_eq!(
            columns(&fixed),
            [
                " C  ", " H  ", " H  ", " H  ", " N  ", " H  ", " H  ", " C  ", "CA2+",
                "    "
            ]
        );
    }

    #[test]
    fn the_rest_of_the_file_is_untouched() {
        let atoms = hmr_system();
        let settled = settle(&atoms).unwrap();
        let mut text = pdb(&CPPTRAJ);
        text.insert_str(
            0,
            "CRYST1   10.000   10.000   10.000  90.00  90.00  90.00 P 1\n",
        );
        text = text.replace("END\n", "TER\nEND\n");
        let (fixed, _) = rewrite(&text, &atoms, &settled).unwrap();
        let other = |t: &str| {
            t.lines()
                .filter(|l| !l.starts_with("ATOM"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(other(&fixed), other(&text));
        for (a, b) in fixed.lines().zip(text.lines()) {
            assert_eq!(a.get(..76), b.get(..76));
        }
    }

    #[test]
    fn an_atom_not_in_the_topology_is_refused() {
        let atoms = hmr_system();
        let settled = settle(&atoms).unwrap();
        let mut swapped = CPPTRAJ;
        swapped.swap(0, 7);
        assert!(rewrite(&pdb(&swapped), &atoms, &settled).is_err());
    }

    #[test]
    fn a_mass_matching_no_element_is_not_guessed() {
        let atoms = parse_psf(&psf(&["CG 1 BEAD BB P5 0.0 72.0"], &[])).unwrap();
        let e = settle(&atoms).unwrap_err();
        assert!(e.contains("matches no element"), "{e}");
    }

    #[test]
    fn the_file_is_left_alone_when_an_element_cannot_be_settled() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let top = dir.path().join("cg.psf");
        let pdb_path = dir.path().join("full.pdb");
        fs::write(&top, psf(&["CG 1 BEAD BB P5 0.0 72.0"], &[]))?;
        let text = pdb(&[("BB", "BEAD", "GE")]);
        fs::write(&pdb_path, &text)?;
        fix_psf_elements(&top, &[&pdb_path])?;
        assert_eq!(fs::read_to_string(&pdb_path)?, text);
        Ok(())
    }

    #[test]
    fn only_a_psf_is_read() {
        assert!(is_psf(Path::new("a/sys.PSF")));
        assert!(!is_psf(Path::new("a/sys.prmtop")));
    }
}
