use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use pretty_assertions::assert_eq;
use std::{fs, path::Path};

const PRG: &str = "mdr-meta";
const INPUT_ROOT: &str = "../libmdrepo/tests/inputs/metadata";

struct RunArgs<'a> {
    args: &'a [String],
    stdout: Option<&'a str>,
    stderr: Option<&'a str>,
}

// --------------------------------------------------
#[test]
fn dies_no_args() -> Result<()> {
    Command::cargo_bin(PRG)?
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
    Ok(())
}

// --------------------------------------------------
fn input_path(filename: &str) -> String {
    let input = Path::new(INPUT_ROOT);
    input.join(filename).to_string_lossy().to_string()
}

// --------------------------------------------------
fn run(args: RunArgs) -> Result<()> {
    let mut cmd = Command::cargo_bin(PRG)?;
    if !args.args.is_empty() {
        cmd.args(args.args);
    }
    dbg!(&cmd);
    let output = cmd.output()?;
    dbg!(&output);

    if let Some(expected_file) = args.stdout {
        let expected = fs::read_to_string(&input_path(expected_file))?;
        let stdout = String::from_utf8(output.stdout).expect("invalid UTF-8");
        assert_eq!(stdout.trim(), expected.trim());
    }

    if let Some(expected_file) = args.stderr {
        let expected = fs::read_to_string(&input_path(expected_file))?;
        let stderr = String::from_utf8(output.stderr).expect("invalid UTF-8");
        assert_eq!(stderr.trim(), expected.trim());
    }

    Ok(())
}

// --------------------------------------------------
#[test]
fn check_ok1() -> Result<()> {
    run(RunArgs {
        args: &["check".to_string(), input_path("ok1.toml")],
        stdout: Some("empty.txt"),
        stderr: None,
    })
}

// --------------------------------------------------
#[test]
fn check_bad1() -> Result<()> {
    run(RunArgs {
        args: &["check".to_string(), input_path("bad1.toml")],
        stdout: Some("expected.bad1.txt"),
        stderr: None,
    })
}

// --------------------------------------------------
#[test]
fn example_toml() -> Result<()> {
    run(RunArgs {
        args: &["example".to_string()],
        stdout: Some("expected.example.toml"),
        stderr: None,
    })
}

// --------------------------------------------------
#[test]
fn example_json() -> Result<()> {
    run(RunArgs {
        args: &["example".to_string(), "-f".to_string(), "json".to_string()],
        stdout: Some("expected.example.json"),
        stderr: None,
    })
}

// --------------------------------------------------
#[test]
fn to_json() -> Result<()> {
    run(RunArgs {
        args: &["to-json".to_string(), input_path("ok1.toml")],
        stdout: Some("ok1.json"),
        stderr: None,
    })
}

// --------------------------------------------------
#[test]
fn to_toml() -> Result<()> {
    run(RunArgs {
        args: &["to-toml".to_string(), input_path("ok1.json")],
        stdout: Some("ok1.toml"),
        stderr: None,
    })
}
