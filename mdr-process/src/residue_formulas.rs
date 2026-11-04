//! Heavy-atom formulas of non-standard chemical components (TPO, SEP, ASJ,
//! ...), for the composition check of a ligand declared by sequence
//! (`sequence::expected_peptide_formula`; Travis 2026-10-02, decision 4(b)).
//! The 22 standard amino acids are in `sequence.rs`; a code in neither is
//! refused.
//!
//! The table is `mdr-process/data/residue-formulas.tsv` (code, CCD type, CCD
//! formula), compiled in. To add codes, from a mdrepo-rs checkout:
//!
//! ```text
//! mdr-process ccd-formula CODE...
//! ```
//!
//! which looks each one up in the PDB's Chemical Component Dictionary
//! (RCSB) and adds it to the table; then rebuild and commit the table.

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use std::{
    collections::BTreeMap, collections::HashMap, fs, path::Path, sync::OnceLock,
};

const TABLE: &str = include_str!("../data/residue-formulas.tsv");

/// Heavy atoms of a CCD formula (`C3 H8 N O6 P`), elements upper case as
/// `sequence::Formula` keys are, hydrogen left out
pub fn heavy_atoms(formula: &str) -> Result<Vec<(String, u32)>> {
    let mut atoms: Vec<(String, u32)> = vec![];
    for part in formula.split_whitespace() {
        let split = part
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(part.len());
        let (element, count) = part.split_at(split);
        if element.is_empty()
            || !element.starts_with(|c: char| c.is_ascii_uppercase())
            || !element.chars().all(|c| c.is_ascii_alphabetic())
        {
            bail!(r#"not a formula: "{formula}""#);
        }
        let count: u32 = if count.is_empty() {
            1
        } else {
            count
                .parse()
                .map_err(|_| anyhow!(r#"not a formula: "{formula}""#))?
        };
        let element = element.to_uppercase();
        if element == "H" {
            continue;
        }
        match atoms.iter_mut().find(|(e, _)| *e == element) {
            Some((_, n)) => *n += count,
            None => atoms.push((element, count)),
        }
    }
    atoms.sort();
    Ok(atoms)
}

/// The table, read once: code -> heavy atoms
fn table() -> &'static HashMap<String, Vec<(String, u32)>> {
    static TABLE_MAP: OnceLock<HashMap<String, Vec<(String, u32)>>> = OnceLock::new();
    TABLE_MAP.get_or_init(|| {
        TABLE
            .lines()
            .filter(|l| {
                !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with("code\t")
            })
            .map(|l| {
                let cols: Vec<&str> = l.split('\t').collect();
                let (code, formula) = (cols[0], cols.get(2).copied().unwrap_or(""));
                // The table's own test checks every row; a bad row here is a
                // build that never passed its tests
                (code.to_string(), heavy_atoms(formula).unwrap_or_default())
            })
            .collect()
    })
}

/// Heavy atoms of a non-standard chemical component as a free molecule, by
/// element, or None when the table does not have it
pub fn component_heavy_atoms(code: &str) -> Option<&'static [(String, u32)]> {
    table().get(code).map(Vec::as_slice)
}

#[derive(Debug, Deserialize)]
struct ChemCompResponse {
    chem_comp: ChemComp,
}

#[derive(Debug, Deserialize)]
struct ChemComp {
    formula: Option<String>,
    #[serde(rename = "type")]
    type_: Option<String>,
}

/// One table line for `code` from RCSB's CCD: code, type, formula
pub fn ccd_line(code: &str) -> Result<String> {
    let code = code.trim().to_uppercase();
    let url = format!("https://data.rcsb.org/rest/v1/core/chemcomp/{code}");
    let resp = reqwest::blocking::get(&url)?;
    if !resp.status().is_success() {
        bail!(r#"{code}: not in the CCD ("{url}": {})"#, resp.status());
    }
    let comp: ChemCompResponse = resp
        .json()
        .map_err(|e| anyhow!("{code}: failed to parse the CCD response: {e}"))?;
    let formula = comp
        .chem_comp
        .formula
        .ok_or_else(|| anyhow!("{code}: the CCD has no formula"))?;
    heavy_atoms(&formula)?;
    Ok(format!(
        "{code}\t{}\t{formula}",
        comp.chem_comp.type_.unwrap_or_default()
    ))
}

/// Add the codes the table at `path` lacks, from the CCD, and rewrite it
/// sorted by code. Returns how many were added
pub fn add_to_table(path: &Path, codes: &[String]) -> Result<usize> {
    let text = fs::read_to_string(path)
        .map_err(|e| anyhow!(r#"cannot read "{}": {e}"#, path.display()))?;
    let mut rows: BTreeMap<String, String> = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("code\t"))
        .map(|l| {
            (
                l.split('\t').next().unwrap_or("").to_string(),
                l.to_string(),
            )
        })
        .collect();
    let mut added = 0;
    for code in codes {
        let code = code.trim().to_uppercase();
        if !rows.contains_key(&code) {
            rows.insert(code.clone(), ccd_line(&code)?);
            added += 1;
        }
    }
    let mut out = String::from("code\ttype\tformula\n");
    for line in rows.values() {
        out.push_str(line);
        out.push('\n');
    }
    fs::write(path, out)?;
    Ok(added)
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn f(pairs: &[(&str, u32)]) -> Vec<(String, u32)> {
        pairs.iter().map(|(e, n)| (e.to_string(), *n)).collect()
    }

    /// A unique scratch file under the system temp dir
    fn scratch(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mdr-residue-formulas-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("table.tsv");
        fs::write(&path, text).unwrap();
        path
    }

    fn rows(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with("code\t"))
            .collect()
    }

    // ---- heavy_atoms --------------------------------------------------

    #[test]
    fn heavy_atoms_drops_hydrogen_and_upper_cases() {
        assert_eq!(
            heavy_atoms("C3 H8 N O6 P").unwrap(),
            f(&[("C", 3), ("N", 1), ("O", 6), ("P", 1)])
        );
    }

    #[test]
    fn heavy_atoms_two_letter_elements() {
        assert_eq!(
            heavy_atoms("C9 H10 Cl N O2").unwrap(),
            f(&[("C", 9), ("CL", 1), ("N", 1), ("O", 2)])
        );
        assert_eq!(
            heavy_atoms("C3 H7 N O2 Se").unwrap(),
            f(&[("C", 3), ("N", 1), ("O", 2), ("SE", 1)])
        );
        assert_eq!(heavy_atoms("C6 H4 Br2").unwrap(), f(&[("BR", 2), ("C", 6)]));
    }

    #[test]
    fn heavy_atoms_an_element_without_a_count_is_one() {
        assert_eq!(heavy_atoms("N").unwrap(), f(&[("N", 1)]));
        assert_eq!(heavy_atoms("C H F3").unwrap(), f(&[("C", 1), ("F", 3)]));
    }

    #[test]
    fn heavy_atoms_repeated_elements_add_up() {
        assert_eq!(heavy_atoms("C2 O C3 O2").unwrap(), f(&[("C", 5), ("O", 3)]));
    }

    #[test]
    fn heavy_atoms_hydrogen_only_is_empty() {
        assert!(heavy_atoms("H2").unwrap().is_empty());
        assert!(heavy_atoms("").unwrap().is_empty());
    }

    #[test]
    fn heavy_atoms_is_sorted_by_element() {
        let got = heavy_atoms("S O N C P").unwrap();
        let mut sorted = got.clone();
        sorted.sort();
        assert_eq!(got, sorted);
    }

    #[test]
    fn heavy_atoms_tolerates_extra_whitespace() {
        assert_eq!(
            heavy_atoms("  C3\tH8  N   O6 P ").unwrap(),
            f(&[("C", 3), ("N", 1), ("O", 6), ("P", 1)])
        );
    }

    #[test]
    fn heavy_atoms_rejects_what_is_not_a_formula() {
        for bad in [
            "C3 H8 x2", "3C", "C3 H8 N-", "c3", "C3 O2x", "C3 O2.5", "C-1",
        ] {
            assert!(heavy_atoms(bad).is_err(), "{bad} should be refused");
        }
    }

    // ---- the compiled table -------------------------------------------

    #[test]
    fn the_table_has_a_header_and_three_columns_per_row() {
        assert!(TABLE.starts_with("code\ttype\tformula\n"));
        for line in rows(TABLE) {
            assert_eq!(line.split('\t').count(), 3, "{line}");
        }
    }

    #[test]
    fn every_table_row_is_a_formula_with_heavy_atoms() {
        for line in rows(TABLE) {
            let formula = line.split('\t').nth(2).unwrap();
            let atoms = heavy_atoms(formula).unwrap();
            assert!(!atoms.is_empty(), "{line}");
            assert!(atoms.iter().all(|(e, _)| e != "H"), "{line}");
        }
    }

    #[test]
    fn table_codes_are_unique_upper_case_and_sorted() {
        let codes: Vec<&str> = rows(TABLE)
            .iter()
            .map(|l| l.split('\t').next().unwrap())
            .collect();
        let unique: HashSet<&str> = codes.iter().copied().collect();
        assert_eq!(unique.len(), codes.len(), "a code appears twice");
        assert!(codes.iter().all(|c| {
            c.chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        }));
        let mut sorted = codes.clone();
        sorted.sort();
        assert_eq!(codes, sorted, "the table is kept sorted by code");
    }

    #[test]
    fn the_table_leaves_the_standard_residues_to_sequence_rs() {
        for code in [
            "ALA", "ARG", "ASN", "ASP", "CYS", "GLN", "GLU", "GLY", "HIS", "ILE",
            "LEU", "LYS", "MET", "PHE", "PRO", "SER", "THR", "TRP", "TYR", "VAL",
            "SEC", "PYL",
        ] {
            assert!(component_heavy_atoms(code).is_none(), "{code} is standard");
        }
    }

    #[test]
    fn the_table_holds_every_code_group_b_needs_most() {
        // The ten most common non-standard codes in the DDD group B matches
        for code in [
            "TPO", "PTR", "SEP", "ASJ", "ALC", "M3L", "56A", "IVA", "ALY", "PCA",
        ] {
            assert!(component_heavy_atoms(code).is_some(), "{code} missing");
        }
    }

    #[test]
    fn known_formulas_from_the_ccd() {
        // Phosphothreonine C4 H10 N O6 P
        assert_eq!(
            component_heavy_atoms("TPO").unwrap(),
            f(&[("C", 4), ("N", 1), ("O", 6), ("P", 1)]).as_slice()
        );
        // Phosphoserine C3 H8 N O6 P
        assert_eq!(
            component_heavy_atoms("SEP").unwrap(),
            f(&[("C", 3), ("N", 1), ("O", 6), ("P", 1)]).as_slice()
        );
        // Phosphotyrosine C9 H12 N O6 P
        assert_eq!(
            component_heavy_atoms("PTR").unwrap(),
            f(&[("C", 9), ("N", 1), ("O", 6), ("P", 1)]).as_slice()
        );
        // Aspartic aldehyde C4 H9 N O3 (the caspase inhibitors, 2ql5)
        assert_eq!(
            component_heavy_atoms("ASJ").unwrap(),
            f(&[("C", 4), ("N", 1), ("O", 3)]).as_slice()
        );
        // Pyroglutamic acid C5 H7 N O3
        assert_eq!(
            component_heavy_atoms("PCA").unwrap(),
            f(&[("C", 5), ("N", 1), ("O", 3)]).as_slice()
        );
        // Trimethyllysine C9 H21 N2 O2
        assert_eq!(
            component_heavy_atoms("M3L").unwrap(),
            f(&[("C", 9), ("N", 2), ("O", 2)]).as_slice()
        );
    }

    #[test]
    fn a_code_not_in_the_table_is_none() {
        assert!(component_heavy_atoms("ZZQ").is_none());
        assert!(component_heavy_atoms("").is_none());
    }

    #[test]
    fn lookups_are_exact_so_callers_pass_upper_case() {
        // sequence.rs upper-cases codes (`canonical`) before the lookup
        assert!(component_heavy_atoms("TPO").is_some());
        assert!(component_heavy_atoms("tpo").is_none());
    }

    #[test]
    fn halogen_and_selenium_codes_keep_their_two_letter_elements() {
        for line in rows(TABLE) {
            let formula = line.split('\t').nth(2).unwrap();
            for (e, _) in heavy_atoms(formula).unwrap() {
                assert!(
                    ["C", "N", "O", "S", "P", "F", "CL", "BR", "I", "SE", "B"]
                        .contains(&e.as_str()),
                    "unexpected element {e} in {line}"
                );
            }
        }
    }

    // ---- add_to_table (no network: only codes already present) --------

    #[test]
    fn add_to_table_adds_nothing_when_every_code_is_present() {
        let path = scratch(
            "present",
            "code\ttype\tformula\nSEP\tL-peptide linking\tC3 H8 N O6 P\n",
        );
        let added = add_to_table(&path, &["SEP".to_string()]).unwrap();
        assert_eq!(added, 0);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "code\ttype\tformula\nSEP\tL-peptide linking\tC3 H8 N O6 P\n"
        );
    }

    #[test]
    fn add_to_table_matches_codes_case_and_space_insensitively() {
        let path = scratch(
            "case",
            "code\ttype\tformula\nTPO\tL-peptide linking\tC4 H10 N O6 P\n",
        );
        let added =
            add_to_table(&path, &[" tpo ".to_string(), "Tpo".to_string()]).unwrap();
        assert_eq!(added, 0);
    }

    #[test]
    fn add_to_table_rewrites_the_table_sorted_with_its_header() {
        let path = scratch(
            "sort",
            "code\ttype\tformula\nTPO\tL-peptide linking\tC4 H10 N O6 P\n\nASJ\tpeptide-like\tC4 H9 N O3\nPTR\tL-peptide linking\tC9 H12 N O6 P\n",
        );
        add_to_table(&path, &["PTR".to_string()]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "code\ttype\tformula\nASJ\tpeptide-like\tC4 H9 N O3\nPTR\tL-peptide linking\tC9 H12 N O6 P\nTPO\tL-peptide linking\tC4 H10 N O6 P\n"
        );
    }

    #[test]
    fn add_to_table_keeps_a_table_with_no_header_working() {
        let path = scratch("noheader", "SEP\tL-peptide linking\tC3 H8 N O6 P\n");
        add_to_table(&path, &["SEP".to_string()]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "code\ttype\tformula\nSEP\tL-peptide linking\tC3 H8 N O6 P\n"
        );
    }

    #[test]
    fn add_to_table_with_no_codes_only_tidies() {
        let path = scratch("none", "code\ttype\tformula\nSEP\tx\tC3\nASJ\ty\tC4\n");
        assert_eq!(add_to_table(&path, &[]).unwrap(), 0);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "code\ttype\tformula\nASJ\ty\tC4\nSEP\tx\tC3\n"
        );
    }

    #[test]
    fn add_to_table_refuses_a_missing_table() {
        let path =
            std::env::temp_dir().join("mdr-residue-formulas-does-not-exist/table.tsv");
        let err = add_to_table(&path, &["SEP".to_string()]).unwrap_err();
        assert!(err.to_string().contains("cannot read"), "{err}");
    }

    #[test]
    #[ignore = "needs the network: cargo test -p mdr-process -- --ignored ccd"]
    fn ccd_line_from_rcsb_agrees_with_the_table() {
        for code in ["SEP", "TPO", "ASJ"] {
            let line = ccd_line(code).unwrap();
            let committed = rows(TABLE)
                .into_iter()
                .find(|l| l.starts_with(&format!("{code}\t")))
                .unwrap();
            assert_eq!(line, committed);
        }
        assert!(ccd_line("ZZQ9X").is_err());
    }

    #[test]
    fn the_compiled_table_is_already_tidy() {
        // What add_to_table would write is what is committed
        let path = scratch("tidy", TABLE);
        add_to_table(&path, &[]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), TABLE);
    }
}
