//! `mdr-process ccd-formula`: adding codes to the residue-formula table.
//! These never reach the network: every code they pass is already in the
//! table, or the command fails before looking anything up.

use anyhow::Result;
use assert_cmd::cargo_bin_cmd;
use std::{fs, path::PathBuf};

const COMMAND: &str = "ccd-formula";
const TABLE: &str = "mdr-process/data/residue-formulas.tsv";

/// A copy of `text` as a table in its own scratch directory
fn scratch(name: &str, text: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir()
        .join(format!("mdr-ccd-formula-cli-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("table.tsv");
    fs::write(&path, text)?;
    Ok(path)
}

// --------------------------------------------------
#[test]
fn needs_at_least_one_code() -> Result<()> {
    cargo_bin_cmd!().args([COMMAND]).assert().failure();
    Ok(())
}

// --------------------------------------------------
#[test]
fn help_names_the_table() -> Result<()> {
    let out = cargo_bin_cmd!().args([COMMAND, "--help"]).output()?;
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout)?;
    assert!(text.contains("residue-formulas.tsv"), "{text}");
    assert!(text.contains("--table"), "{text}");
    Ok(())
}

// --------------------------------------------------
#[test]
fn is_listed_among_the_commands() -> Result<()> {
    let out = cargo_bin_cmd!().args(["--help"]).output()?;
    let text = String::from_utf8(out.stdout)?;
    assert!(text.contains(COMMAND), "{text}");
    Ok(())
}

// --------------------------------------------------
#[test]
fn dies_on_a_missing_table() -> Result<()> {
    let missing = std::env::temp_dir().join("mdr-ccd-formula-cli-missing/table.tsv");
    cargo_bin_cmd!()
        .args([COMMAND, "SEP", "--table"])
        .arg(&missing)
        .assert()
        .failure();
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_code_already_present_leaves_the_table_as_it_was() -> Result<()> {
    let text = "code\ttype\tformula\nSEP\tL-peptide linking\tC3 H8 N O6 P\n";
    let path = scratch("present", text)?;
    cargo_bin_cmd!()
        .args([COMMAND, "SEP", "--table"])
        .arg(&path)
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path)?, text);
    Ok(())
}

// --------------------------------------------------
#[test]
fn codes_are_matched_whatever_their_case() -> Result<()> {
    let text = "code\ttype\tformula\nTPO\tL-peptide linking\tC4 H10 N O6 P\n";
    let path = scratch("case", text)?;
    cargo_bin_cmd!()
        .args([COMMAND, "tpo", "Tpo", "-t"])
        .arg(&path)
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path)?, text);
    Ok(())
}

// --------------------------------------------------
#[test]
fn the_table_comes_back_sorted() -> Result<()> {
    let path = scratch(
        "sorted",
        "code\ttype\tformula\nTPO\ta\tC4 H10 N O6 P\nASJ\tb\tC4 H9 N O3\n",
    )?;
    cargo_bin_cmd!()
        .args([COMMAND, "ASJ", "--table"])
        .arg(&path)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(&path)?,
        "code\ttype\tformula\nASJ\tb\tC4 H9 N O3\nTPO\ta\tC4 H10 N O6 P\n"
    );
    Ok(())
}

// --------------------------------------------------
#[test]
fn every_code_in_the_committed_table_is_already_present() -> Result<()> {
    // Rerunning the command over the table's own codes changes nothing:
    // the committed file is what the command writes
    let committed = fs::read_to_string(format!("../{TABLE}"))?;
    let codes: Vec<&str> = committed
        .lines()
        .skip(1)
        .filter_map(|l| l.split('\t').next())
        .collect();
    assert!(codes.len() > 200, "{} codes", codes.len());
    let path = scratch("committed", &committed)?;
    cargo_bin_cmd!()
        .arg(COMMAND)
        .args(&codes)
        .arg("--table")
        .arg(&path)
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path)?, committed);
    Ok(())
}
