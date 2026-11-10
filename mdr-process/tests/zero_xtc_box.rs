//! `mdr-process zero-xtc-box IN [-o FILE]`: report, or write a copy whose
//! boxes are zero. The rules are unit-tested in src/xtc_box.rs; these check
//! the command line: exit codes, what is written, what is refused.

use anyhow::Result;
use assert_cmd::cargo_bin_cmd;
use std::{
    fs,
    path::{Path, PathBuf},
};

const COMMAND: &str = "zero-xtc-box";

type Mat3 = [[f32; 3]; 3];
const CUBE: Mat3 = [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 5.0]];
const INF: Mat3 = [
    [f32::INFINITY, 0.0, 0.0],
    [0.0, f32::NEG_INFINITY, 0.0],
    [0.0, 0.0, 0.0],
];

/// 12 atoms (compressed frames), one frame per box
fn xtc(path: &Path, boxes: &[Mat3]) -> Result<PathBuf> {
    let mut w = molly::XTCWriter::create(path)?;
    for (i, b) in boxes.iter().enumerate() {
        w.write_frame(&molly::Frame {
            step: i as u32,
            time: 10.0 * i as f32,
            boxvec: b
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            precision: 1000.0,
            positions: (0..36).map(|k| 0.1 * k as f32).collect(),
        })?;
    }
    Ok(path.to_path_buf())
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// --------------------------------------------------
#[test]
fn needs_an_input() -> Result<()> {
    cargo_bin_cmd!().arg(COMMAND).assert().failure();
    Ok(())
}

// --------------------------------------------------
#[test]
fn report_only_writes_nothing() -> Result<()> {
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[INF, INF])?;
    let before = fs::read(&i)?;
    let out = cargo_bin_cmd!().arg(COMMAND).arg(&i).output()?;
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("2 frames\t2 to zero\tnon-finite 2"), "{text}");
    assert!(text.contains("(report only)"), "{text}");
    assert_eq!(fs::read(&i)?, before);
    assert_eq!(fs::read_dir(d.path())?.count(), 1);
    Ok(())
}

// --------------------------------------------------
#[test]
fn writes_a_zeroed_copy() -> Result<()> {
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[INF, INF, INF])?;
    let o = d.path().join("fixed.xtc");
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(&i)
        .arg("-o")
        .arg(&o)
        .output()?;
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("3 box(es) zeroed"),
        "{}",
        stdout(&out)
    );
    assert_eq!(fs::metadata(&i)?.len(), fs::metadata(&o)?.len());
    // The copy now passes the output checks; the original still fails
    let ok = |dir: &Path| -> Result<bool> {
        Ok(cargo_bin_cmd!()
            .args(["check-outputs", "-q"])
            .arg(dir)
            .output()?
            .status
            .success())
    };
    let (a, b) = (d.path().join("a"), d.path().join("b"));
    fs::create_dir_all(&a)?;
    fs::create_dir_all(&b)?;
    fs::copy(&i, a.join("full.xtc"))?;
    fs::copy(&o, b.join("full.xtc"))?;
    assert!(!ok(&a)?);
    assert!(ok(&b)?);
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_real_cell_is_refused() -> Result<()> {
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[CUBE, INF])?;
    let o = d.path().join("fixed.xtc");
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(&i)
        .arg("-o")
        .arg(&o)
        .output()?;
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("real-looking cell"),
        "{}",
        stderr(&out)
    );
    assert!(!o.exists());
    Ok(())
}

// --------------------------------------------------
#[test]
fn an_existing_output_is_kept() -> Result<()> {
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[INF])?;
    let o = d.path().join("fixed.xtc");
    fs::write(&o, "keep")?;
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(&i)
        .arg("-o")
        .arg(&o)
        .output()?;
    assert!(!out.status.success());
    assert!(stderr(&out).contains("already exists"), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(&o)?, "keep");
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_missing_input_is_an_error() -> Result<()> {
    let d = tempfile::tempdir()?;
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(d.path().join("nope.xtc"))
        .output()?;
    assert!(!out.status.success());
    Ok(())
}

// --------------------------------------------------
#[test]
fn json_report() -> Result<()> {
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[INF, CUBE])?;
    let j = d.path().join("r.json");
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg(&i)
        .arg("--json")
        .arg(&j)
        .output()?;
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&j)?)?;
    assert_eq!(v[0]["name"], "full.xtc");
    assert_eq!(v[0]["frames"], 2);
    assert_eq!(v[0]["to_zero"], 2);
    assert_eq!(v[0]["kinds"]["cell"], 1);
    assert_eq!(v[0]["kinds"]["non-finite"], 1);
    Ok(())
}

// --------------------------------------------------
#[test]
fn help_says_it_never_writes_the_input() -> Result<()> {
    let out = cargo_bin_cmd!().args([COMMAND, "--help"]).output()?;
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("<IN>"), "{text}");
    assert!(text.contains("-o, --output <FILE>"), "{text}");
    assert!(text.contains("Never writes IN"), "{text}");
    Ok(())
}

// --------------------------------------------------
#[test]
fn a_second_positional_is_refused() -> Result<()> {
    // The output is named (-o/--output), never a bare second argument
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[INF])?;
    let o = d.path().join("fixed.xtc");
    let out = cargo_bin_cmd!().arg(COMMAND).arg(&i).arg(&o).output()?;
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("unexpected argument"),
        "{}",
        stderr(&out)
    );
    assert!(!o.exists());
    Ok(())
}

// --------------------------------------------------
#[test]
fn long_output_flag_works() -> Result<()> {
    let d = tempfile::tempdir()?;
    let i = xtc(&d.path().join("full.xtc"), &[INF, INF])?;
    let o = d.path().join("fixed.xtc");
    let out = cargo_bin_cmd!()
        .arg(COMMAND)
        .arg("--output")
        .arg(&o)
        .arg(&i)
        .output()?;
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("2 box(es) zeroed"),
        "{}",
        stdout(&out)
    );
    assert!(o.exists());
    Ok(())
}
