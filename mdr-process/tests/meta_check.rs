//! `mdr-process meta-check`: the text output callers already parse, and the
//! `--json` and `--agent-imported` options (mdrepo-rs#46; Danny's point 5).
//!
//! The inputs in `tests/inputs/agent/` are the TOML mdrepo-agent wrote for
//! Zenodo 8420199 (staging MDR00000530, fetched 2026-11-05): as written,
//! declared CUSTOM to get past the format check (mdrepo-agent#88); declared as
//! the GROMACS 2020 run it is; and without software, temperature or ORCID. The
//! last two are edits of the first. `top-without-coordinates.toml` gives it a
//! GROMACS `.top` and no `.tpr`/`.gro`, the shape of Zenodo 3975394.

use anyhow::Result;
use assert_cmd::cargo_bin_cmd;
use serde_json::Value;

const COMMAND: &str = "meta-check";
const CUSTOM: &str = "tests/inputs/agent/8420199-custom.toml";
const GROMACS: &str = "tests/inputs/agent/8420199-gromacs.toml";
const NO_SOFTWARE: &str = "tests/inputs/agent/8420199-no-software.toml";
const TOP: &str = "tests/inputs/agent/top-without-coordinates.toml";
const BROKEN: &str = "tests/inputs/agent/broken.toml";
const MISMATCH: &str = "software_name is \"GROMACS\" but structure/topology/trajectory \
    extensions (.pdb, .psf, .xtc, .xtc, .xtc) match ACEMD or CHARMM or NAMD instead";

/// stdout and stderr of `meta-check` with `args`; it must exit 0 whatever
/// it finds, as mdrepo-agent expects
fn run(args: &[&str]) -> Result<(String, String)> {
    let out = cargo_bin_cmd!().arg(COMMAND).args(args).output()?;
    assert!(
        out.status.success(),
        "meta-check {args:?} exited {:?}",
        out.status
    );
    Ok((
        String::from_utf8(out.stdout)?,
        String::from_utf8(out.stderr)?,
    ))
}

/// The lines of text output that are not the log (which goes to stdout too)
fn complaints(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .collect()
}

/// `--json` output parsed; stdout must be the JSON object and nothing else
fn json(args: &[&str]) -> Result<Value> {
    let mut full = vec!["--json"];
    full.extend_from_slice(args);
    let (stdout, _) = run(&full)?;
    Ok(serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not one JSON object ({e}):\n{stdout}")))
}

fn codes(report: &Value, key: &str) -> Vec<String> {
    report[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap().to_string())
        .collect()
}

// --- text output, as before ---

#[test]
fn text_clean_file_prints_no_complaints() -> Result<()> {
    let (stdout, _) = run(&[CUSTOM])?;
    assert!(complaints(&stdout).is_empty(), "{stdout}");
    Ok(())
}

#[test]
fn text_mismatch_is_one_line_as_before() -> Result<()> {
    let (stdout, _) = run(&[GROMACS])?;
    assert_eq!(complaints(&stdout), vec![MISMATCH]);
    Ok(())
}

#[test]
fn text_agent_mismatch_prints_no_complaint() -> Result<()> {
    // a warning goes to the log, never among the complaint lines
    let (stdout, _) = run(&["--agent-imported", GROMACS])?;
    assert!(complaints(&stdout).is_empty(), "{stdout}");
    assert!(
        stdout.contains("Metadata warning (format_mismatch)"),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn text_missing_fields_are_required_for_a_person() -> Result<()> {
    let (stdout, _) = run(&[NO_SOFTWARE])?;
    assert_eq!(
        complaints(&stdout),
        // field errors are sorted by message, as they always have been
        vec![
            "lead_contributor_orcid: required",
            "software_name: required",
            "software_version: required",
            "temperature_kelvin: required",
        ]
    );
    Ok(())
}

#[test]
fn text_parse_failure_reads_as_before() -> Result<()> {
    // the TOML error runs over several lines (the bad line, a caret)
    let (stdout, _) = run(&[BROKEN])?;
    let lines = complaints(&stdout);
    assert!(
        lines[0].starts_with("Unable to parse tests/inputs/agent/broken.toml"),
        "{stdout}"
    );
    Ok(())
}

// --- --json ---

#[test]
fn json_clean_file() -> Result<()> {
    let report = json(&[CUSTOM])?;
    assert_eq!(report["ok"], true);
    assert_eq!(report["errors"], serde_json::json!([]));
    assert_eq!(report["warnings"], serde_json::json!([]));
    Ok(())
}

#[test]
fn json_mismatch_is_an_error_for_a_person() -> Result<()> {
    let report = json(&[GROMACS])?;
    assert_eq!(report["ok"], false);
    assert_eq!(
        report["errors"],
        serde_json::json!([{
            "level": "error",
            "field": "software_name",
            "code": "format_mismatch",
            "message": MISMATCH,
        }])
    );
    assert_eq!(report["warnings"], serde_json::json!([]));
    Ok(())
}

#[test]
fn json_mismatch_is_a_warning_for_an_agent() -> Result<()> {
    let report = json(&["--agent-imported", GROMACS])?;
    assert_eq!(report["ok"], true);
    assert_eq!(report["errors"], serde_json::json!([]));
    assert_eq!(codes(&report, "warnings"), vec!["format_mismatch"]);
    assert_eq!(report["warnings"][0]["level"], "warning");
    assert_eq!(report["warnings"][0]["message"], MISMATCH);
    Ok(())
}

#[test]
fn json_agent_may_leave_out_software_temperature_and_orcid() -> Result<()> {
    let report = json(&["--agent-imported", NO_SOFTWARE])?;
    assert_eq!(report["ok"], true, "{report}");
    // and a person may not
    let report = json(&[NO_SOFTWARE])?;
    assert_eq!(report["ok"], false);
    assert_eq!(codes(&report, "errors"), vec!["required"; 4]);
    let fields: Vec<&str> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["field"].as_str().unwrap())
        .collect();
    assert_eq!(
        fields,
        vec![
            "lead_contributor_orcid",
            "software_name",
            "software_version",
            "temperature_kelvin"
        ]
    );
    Ok(())
}

#[test]
fn json_top_without_coordinates_is_refused_even_for_an_agent() -> Result<()> {
    for args in [vec![TOP], vec!["--agent-imported", TOP]] {
        let report = json(&args)?;
        assert_eq!(report["ok"], false, "{args:?}");
        assert!(
            codes(&report, "errors")
                .contains(&"gromacs_top_needs_coordinates".to_string()),
            "{report}"
        );
    }
    Ok(())
}

#[test]
fn json_parse_failure_is_one_error_with_code_parse() -> Result<()> {
    let report = json(&[BROKEN])?;
    assert_eq!(report["ok"], false);
    assert_eq!(codes(&report, "errors"), vec!["parse"]);
    assert_eq!(report["errors"][0]["field"], Value::Null);
    assert!(
        report["errors"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("Unable to parse")
    );
    Ok(())
}

#[test]
fn json_missing_file_is_a_parse_error() -> Result<()> {
    let report = json(&["tests/inputs/agent/no-such-file.toml"])?;
    assert_eq!(report["ok"], false);
    assert_eq!(codes(&report, "errors"), vec!["parse"]);
    Ok(())
}

#[test]
fn json_no_id_still_applies() -> Result<()> {
    let toml = std::fs::read_to_string(CUSTOM)?.replace("pdb_id = \"6SO5\"\n", "");
    let dir = std::env::temp_dir()
        .join(format!("mdr-meta-check-noid-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("mdrepo-metadata.toml");
    std::fs::write(&path, toml)?;
    let path = path.to_str().unwrap();
    assert_eq!(codes(&json(&[path])?, "errors"), vec!["missing_ids"]);
    assert_eq!(json(&["--no-id", path])?["ok"], true);
    Ok(())
}

#[test]
fn json_log_goes_to_stderr() -> Result<()> {
    let (stdout, stderr) = run(&["--json", CUSTOM])?;
    assert!(stdout.trim_start().starts_with('{'), "{stdout}");
    assert!(!stdout.contains("INFO"), "{stdout}");
    assert!(
        stderr.contains("mdr-process"),
        "the build line is logged: {stderr}"
    );
    Ok(())
}

// --- help ---

#[test]
fn help_lists_the_new_options() -> Result<()> {
    let (stdout, _) = run(&["--help"])?;
    assert!(stdout.contains("--json"), "{stdout}");
    assert!(stdout.contains("--agent-imported"), "{stdout}");
    Ok(())
}

#[test]
fn process_help_lists_agent_imported() -> Result<()> {
    let out = cargo_bin_cmd!().args(["process", "--help"]).output()?;
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout)?;
    assert!(text.contains("--agent-imported"), "{text}");
    assert!(text.contains("mdrepo_admin"), "{text}");
    Ok(())
}
