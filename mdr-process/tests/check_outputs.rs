//! `mdr-process check-outputs`: one folder per call. Several folders used
//! to be one simulation's file set (first file of a name won), so
//! `check-outputs rep_*` checked rep_1 only and passed a run whose rep_2 had
//! an infinite box; a missing or empty folder passed too.

use anyhow::Result;
use assert_cmd::cargo_bin_cmd;
use std::{
    fs,
    path::{Path, PathBuf},
};

const COMMAND: &str = "check-outputs";

type Mat3 = [[f32; 3]; 3];
const CUBE: Mat3 = [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 5.0]];
const INF: Mat3 = [
    [f32::NEG_INFINITY, 0.0, 0.0],
    [0.0, -214.5, f32::NEG_INFINITY],
    [0.0, 0.0, 0.0],
];

/// `dir`/full.xtc: 3 atoms, 3 frames 10 ps apart, every frame boxed `b`
fn replicate(dir: &Path, b: Mat3) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut w = molly::XTCWriter::create(dir.join("full.xtc"))?;
    for i in 0..3u32 {
        w.write_frame(&molly::Frame {
            step: i,
            time: 10.0 * i as f32,
            boxvec: b
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            precision: 1000.0,
            positions: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9],
        })?;
    }
    Ok(dir.to_path_buf())
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// --------------------------------------------------
#[test]
fn needs_a_folder() -> Result<()> {
    cargo_bin_cmd!().args([COMMAND]).assert().failure();
    Ok(())
}

// --------------------------------------------------
#[test]
fn two_folders_are_refused() -> Result<()> {
    let d = tempfile::tempdir()?;
    let good = replicate(&d.path().join("rep_1"), CUBE)?;
    let bad = replicate(&d.path().join("rep_2"), INF)?;
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(&good)
        .arg(&bad)
        .output()?;
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("unexpected argument"),
        "{}",
        stderr(&out)
    );
    // Nothing was checked, so no summary line
    assert!(!stdout(&out).contains("fail,"), "{}", stdout(&out));
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_good_replicate_passes() -> Result<()> {
    let d = tempfile::tempdir()?;
    let good = replicate(&d.path().join("rep_1"), CUBE)?;
    let out = cargo_bin_cmd!().arg(COMMAND).arg(&good).output()?;
    assert!(out.status.success(), "{}{}", stdout(&out), stderr(&out));
    assert!(stdout(&out).contains("0 fail, 0 warn"), "{}", stdout(&out));
    Ok(())
}

// --------------------------------------------------
#[test]
fn an_infinite_box_fails() -> Result<()> {
    let d = tempfile::tempdir()?;
    let bad = replicate(&d.path().join("rep_2"), INF)?;
    let out = cargo_bin_cmd!().arg(COMMAND).arg(&bad).output()?;
    assert!(!out.status.success());
    let text = stdout(&out);
    assert!(text.contains("FAIL box"), "{text}");
    assert!(text.contains("non-finite"), "{text}");
    Ok(())
}

// --------------------------------------------------
#[test]
fn each_replicate_by_its_own_call() -> Result<()> {
    // The loop a person should write instead of `check-outputs rep_*`
    let d = tempfile::tempdir()?;
    replicate(&d.path().join("rep_1"), CUBE)?;
    replicate(&d.path().join("rep_2"), INF)?;
    replicate(&d.path().join("rep_3"), CUBE)?;
    let mut failed = vec![];
    for r in ["rep_1", "rep_2", "rep_3"] {
        let ok = cargo_bin_cmd!()
            .arg(COMMAND)
            .arg(d.path().join(r))
            .output()?
            .status
            .success();
        if !ok {
            failed.push(r);
        }
    }
    assert_eq!(failed, vec!["rep_2"]);
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_missing_folder_is_an_error() -> Result<()> {
    let d = tempfile::tempdir()?;
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(d.path().join("nope"))
        .output()?;
    assert!(!out.status.success());
    assert!(stderr(&out).contains("not a folder"), "{}", stderr(&out));
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_file_instead_of_a_folder_is_an_error() -> Result<()> {
    let d = tempfile::tempdir()?;
    let good = replicate(&d.path().join("rep_1"), CUBE)?;
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(good.join("full.xtc"))
        .output()?;
    assert!(!out.status.success());
    assert!(stderr(&out).contains("not a folder"), "{}", stderr(&out));
    Ok(())
}

// --------------------------------------------------
#[test]
fn an_empty_folder_fails() -> Result<()> {
    let d = tempfile::tempdir()?;
    let out = cargo_bin_cmd!().arg(COMMAND).arg(d.path()).output()?;
    assert!(!out.status.success());
    let text = stdout(&out);
    assert!(text.contains("FAIL files"), "{text}");
    assert!(text.contains("1 fail, 0 warn"), "{text}");
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_folder_of_only_replicate_folders_fails() -> Result<()> {
    // Files one level down are not found. (A real processed/ also holds
    // top-level copies of the published files, and those are checked.)
    let d = tempfile::tempdir()?;
    replicate(&d.path().join("rep_1"), CUBE)?;
    let out = cargo_bin_cmd!().arg(COMMAND).arg(d.path()).output()?;
    assert!(!out.status.success());
    assert!(stdout(&out).contains("FAIL files"), "{}", stdout(&out));
    Ok(())
}

// --------------------------------------------------
#[test]
fn json_holds_the_empty_folder_failure() -> Result<()> {
    let d = tempfile::tempdir()?;
    let empty = d.path().join("empty");
    fs::create_dir_all(&empty)?;
    let json = d.path().join("out.json");
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(&empty)
        .arg("--json")
        .arg(&json)
        .output()?;
    assert!(!out.status.success());
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json)?)?;
    assert_eq!(v[0]["check"], "files");
    assert_eq!(v[0]["level"], "fail");
    Ok(())
}

// --------------------------------------------------
#[test]
fn quiet_prints_only_the_failure() -> Result<()> {
    let d = tempfile::tempdir()?;
    let bad = replicate(&d.path().join("rep_2"), INF)?;
    let out = cargo_bin_cmd!().arg(COMMAND).arg(&bad).arg("-q").output()?;
    let text = stdout(&out);
    // The startup log line says INFO; the findings must not
    for check in ["box", "time", "atoms"] {
        assert!(!text.contains(&format!("INFO {check}")), "{text}");
    }
    assert!(text.contains("FAIL box"), "{text}");
    Ok(())
}

// --------------------------------------------------
#[test]
fn help_says_one_folder_per_replicate() -> Result<()> {
    let out = cargo_bin_cmd!().args([COMMAND, "--help"]).output()?;
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("<DIR>"), "{text}");
    assert!(!text.contains("<DIR>..."), "{text}");
    assert!(text.contains("its own call"), "{text}");
    Ok(())
}

// --------------------------------------------------
/// Real processed replicates, when present (the ddd host): every folder in
/// MDR_CHECK_OUTPUTS_BAD (colon-separated) must fail, e.g. 105222's pre-#44
/// rep_2 in /mdrepotmp/work-simproc-retire-2026-11-06/run/new/
#[test]
#[ignore = "needs processed folders in MDR_CHECK_OUTPUTS_BAD"]
fn real_bad_replicates_fail() -> Result<()> {
    let dirs = std::env::var("MDR_CHECK_OUTPUTS_BAD")?;
    for d in dirs.split(':') {
        let out = cargo_bin_cmd!().args([COMMAND, d]).output()?;
        assert!(!out.status.success(), "{d} passed:\n{}", stdout(&out));
    }
    Ok(())
}
