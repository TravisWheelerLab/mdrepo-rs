use anyhow::Result;
use assert_cmd::cargo_bin_cmd;

const COMMAND: &str = "validate";
const EMPTY_DIR: &str = "./tests/inputs/empty_dir";

// --------------------------------------------------
#[test]
fn dies_empty_dir() -> Result<()> {
    cargo_bin_cmd!()
        .args(&[COMMAND, EMPTY_DIR])
        .assert()
        .failure();
    Ok(())
}

// --- validate --agent-imported (mdrepo-agent#105) ---
//
// The TOML mdrepo-agent wrote for Zenodo 8420199 (GROMACS with a generated
// .psf), in a directory with one-byte stand-ins for its files: validate
// checks only that they exist and are not all empty.

const GROMACS: &str = "tests/inputs/agent/8420199-gromacs.toml";
const NO_SOFTWARE: &str = "tests/inputs/agent/8420199-no-software.toml";
const AGENT_FILES: [&str; 5] = [
    "6-chol_PC.pdb",
    "6-chol_PC.psf",
    "noW_0.xtc",
    "noW_1.xtc",
    "noW_2.xtc",
];

/// `validate` looks up uv before it reads anything (it canonicalizes ligand
/// SMILES with it), so skip where there is none, as CI has none
fn have_uv() -> bool {
    std::process::Command::new("uv")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn agent_dir(toml: &str) -> Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    std::fs::copy(toml, dir.path().join("mdrepo-metadata.toml"))?;
    for name in AGENT_FILES {
        std::fs::write(dir.path().join(name), "x")?;
    }
    Ok(dir)
}

/// validate prints "<dir>: OK" for a clean directory, or "OK" on its own line
/// when a warning was logged (to stdout) after the directory name
fn passed(stdout: &str) -> bool {
    stdout
        .lines()
        .last()
        .is_some_and(|line| line == "OK" || line.ends_with(": OK"))
}

fn validate(args: &[&str], dir: &tempfile::TempDir) -> Result<String> {
    let out = cargo_bin_cmd!()
        .env("SCRIPT_DIR", ".")
        .arg(COMMAND)
        .args(args)
        .arg(dir.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    Ok(String::from_utf8(out)?)
}

#[test]
fn mismatched_formats_are_refused_without_the_flag() -> Result<()> {
    if !have_uv() {
        eprintln!("skipped: no uv");
        return Ok(());
    }
    let dir = agent_dir(GROMACS)?;
    let stdout = validate(&[], &dir)?;
    assert!(!passed(&stdout), "{stdout}");
    assert!(stdout.contains("1 error"), "{stdout}");
    Ok(())
}

#[test]
fn mismatched_formats_pass_for_an_agent() -> Result<()> {
    if !have_uv() {
        eprintln!("skipped: no uv");
        return Ok(());
    }
    let dir = agent_dir(GROMACS)?;
    let stdout = validate(&["--agent-imported"], &dir)?;
    assert!(passed(&stdout), "{stdout}");
    Ok(())
}

#[test]
fn an_agent_may_leave_out_software_temperature_and_orcid() -> Result<()> {
    if !have_uv() {
        eprintln!("skipped: no uv");
        return Ok(());
    }
    let dir = agent_dir(NO_SOFTWARE)?;
    assert!(!passed(&validate(&[], &dir)?));
    let stdout = validate(&["--agent-imported"], &dir)?;
    assert!(passed(&stdout), "{stdout}");
    Ok(())
}
