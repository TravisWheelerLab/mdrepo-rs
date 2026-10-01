//! Records the commit `mdr-process` was built from, for `--version` and the
//! first log line, so every host can say which build it runs. `mdr-process`
//! has no tags or GitHub releases (only `mdr-meta` does; see RELEASE.md), so
//! the version number alone cannot tell two builds apart.
//!
//! The commit is `git rev-parse --short HEAD`, with `-dirty` when the working
//! tree has uncommitted changes, and `unknown` when there is no git (a build
//! from a copied tree).

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let commit = match git(&["rev-parse", "--short", "HEAD"]) {
        Some(hash) => {
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
                .is_some_and(|s| !s.is_empty());
            if dirty { format!("{hash}-dirty") } else { hash }
        }
        None => "unknown".to_string(),
    };
    println!("cargo:rustc-env=MDR_PROCESS_COMMIT={commit}");

    // Rerun when the commit or the tracked files change. Naming any path
    // turns off cargo's default of rerunning on every change in this
    // package, so the workspace sources this binary is built from are listed.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={git_dir}/index");
        println!("cargo:rerun-if-changed={git_dir}/refs/heads");
    }
    for path in ["src", "../libmdrepo/src", "../mdr-db/src", "Cargo.toml"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
