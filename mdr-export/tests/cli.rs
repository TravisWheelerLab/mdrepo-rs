use std::process::Command;

/// `--version` names the version and the commit, as `mdr-process` does, so
/// a host can say which build its nightly export runs
#[test]
fn version_names_the_commit() {
    let out = Command::new(env!("CARGO_BIN_EXE_mdr-export"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let want = format!("mdr-export {} (", env!("CARGO_PKG_VERSION"));
    assert!(text.starts_with(&want), "{text}");
    assert!(text.trim_end().ends_with(')'), "{text}");
}
