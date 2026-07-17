use anyhow::{bail, Result};
use libmdrepo::metadata::{Meta, MetaCheckOptions};
use log::debug;
use std::{fs, path::Path, process::Command};

// --------------------------------------------------
/// Read the metadata and canonicalize its ligand SMILES in memory, so both the
/// validator (`purr`) and everything downstream see the canonical forms without
/// the on-disk TOML ever being rewritten. OpenBabel accepts non-standard
/// notation such as `[N+H3]` that `purr` rejects and normalizes it to `[NH3+]`,
/// so canonicalizing before `validate_meta` keeps the validator happy while the
/// submitter's file is preserved as sent. Shells out only when the metadata
/// actually carries ligands.
pub fn load_canonical_meta(meta_path: &Path, script_dir: &Path, uv: &Path) -> Result<Meta> {
    let mut meta = Meta::from_file(meta_path)?;

    let smiles: Vec<String> = match meta.ligands.as_ref() {
        Some(ligands) if !ligands.is_empty() => {
            ligands.iter().map(|l| l.smiles.clone()).collect()
        }
        _ => return Ok(meta),
    };

    let canonical = canonicalize_smiles(&smiles, script_dir, uv)?;
    for (ligand, canon) in meta.ligands.as_mut().unwrap().iter_mut().zip(canonical) {
        ligand.smiles = canon;
    }
    Ok(meta)
}

// --------------------------------------------------
/// Batch-canonicalize SMILES with OpenBabel via `canonicalize_smiles.py`,
/// returning the canonical forms in input order. One `uv` spawn covers the whole
/// batch (the OpenBabel import alone costs ~1s). An unparsable SMILES makes the
/// script exit non-zero, surfaced here as a submitter-facing error.
fn canonicalize_smiles(smiles: &[String], script_dir: &Path, uv: &Path) -> Result<Vec<String>> {
    let script = script_dir.join("canonicalize_smiles.py");
    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir)
        .arg("run")
        .arg(&script)
        .args(smiles);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(
            "{} failed: {}",
            script.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let canonical: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.to_string())
        .collect();
    if canonical.len() != smiles.len() {
        bail!(
            "canonicalize_smiles.py returned {} line(s) for {} SMILES",
            canonical.len(),
            smiles.len()
        );
    }
    Ok(canonical)
}

// --------------------------------------------------
/// Check that a directory holds a processable simulation for the given (already
/// loaded) metadata: the metadata passes its own checks, and the files it
/// references are present and not all empty. This says nothing about how the
/// directory got here -- verifying an upload against its manifest is
/// `ticket::check_manifest`, and reading/canonicalizing the metadata is
/// `load_canonical_meta`.
pub fn validate_meta(
    dir: &Path,
    meta: &Meta,
    meta_check_opts: Option<MetaCheckOptions>,
) -> Result<Vec<String>> {
    if !dir.is_dir() {
        bail!(r#"Invalid directory "{}""#, dir.display())
    }

    debug!(r#"Processing input directory "{}""#, dir.display());

    let num_files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .count();
    debug!(
        "Found {num_files} file{}",
        if num_files == 1 { "" } else { "s" }
    );

    if num_files == 0 {
        bail!(r#""{}" is empty!"#, dir.display());
    }

    let mut errors = meta.check(meta_check_opts);

    // Cross-check the metadata's claims against the directory: every file it
    // references must exist. A missing file is reported like any other error
    // rather than aborting, so the caller sees the whole picture at once.
    let mut total_file_size = 0;
    for filename in &meta.all_filenames() {
        match dir.join(filename).metadata() {
            Ok(metadata) => total_file_size += metadata.len(),
            Err(e) => errors.push(format!(
                r#"Missing file "{filename}" referenced by metadata: {e}"#
            )),
        }
    }

    if total_file_size == 0 {
        errors.push("All local files are empty!".to_string());
    }

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    const MINIMAL_TOML: &str = r#"
        lead_contributor_orcid = "0000-0000-0000-0000"
        trajectory_file_names = ["sim.xtc"]
        structure_file_name = "sim.pdb"
        topology_file_name = "sim.top"
        temperature_kelvin = 300
        integration_timestep_fs = 2
        short_description = "A test simulation"
        software_name = "GROMACS"
        software_version = "2023"
        pdb_id = "1ABC"
        [water]
        model = "TIP3P"
        density_kg_m3 = 1000.0
    "#;

    fn minimal_meta() -> Meta {
        Meta::from_toml(MINIMAL_TOML).unwrap()
    }

    fn write_minimal_dir(dir: &Path) {
        fs::write(dir.join("mdrepo-metadata.toml"), MINIMAL_TOML).unwrap();
        fs::write(dir.join("sim.xtc"), b"trajectory").unwrap();
        fs::write(dir.join("sim.pdb"), b"structure").unwrap();
        fs::write(dir.join("sim.top"), b"topology").unwrap();
    }

    // A path that cannot exist, used to prove `load_canonical_meta` does not
    // spawn `uv` when there are no ligands to canonicalize -- a spawn attempt
    // would fail and turn these Ok cases into errors.
    fn bogus_uv() -> &'static Path {
        Path::new("/nonexistent/uv")
    }

    #[test]
    fn rejects_nonexistent_dir() {
        assert!(validate_meta(Path::new("/nonexistent/path"), &minimal_meta(), None).is_err());
    }

    #[test]
    fn rejects_empty_dir() {
        let dir = tempdir().unwrap();
        let err = validate_meta(dir.path(), &minimal_meta(), None).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn load_canonical_meta_rejects_missing_toml() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("mdrepo-metadata.toml");
        assert!(load_canonical_meta(&missing, Path::new("."), bogus_uv()).is_err());
    }

    #[test]
    fn load_canonical_meta_rejects_invalid_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mdrepo-metadata.toml");
        fs::write(&path, "not {{ valid").unwrap();
        assert!(load_canonical_meta(&path, Path::new("."), bogus_uv()).is_err());
    }

    #[test]
    fn accepts_valid_submission() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        // No ligands -> no OpenBabel spawn even though `uv` is bogus.
        let meta = load_canonical_meta(
            &dir.path().join("mdrepo-metadata.toml"),
            Path::new("."),
            bogus_uv(),
        )
        .unwrap();
        assert!(validate_meta(dir.path(), &meta, None).is_ok());
    }

    #[test]
    fn reports_file_referenced_by_metadata_but_missing() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        fs::remove_file(dir.path().join("sim.xtc")).unwrap();
        let errors = validate_meta(dir.path(), &minimal_meta(), None).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing") && e.contains("sim.xtc")));
    }

    #[test]
    fn reports_all_empty_files() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        for name in ["sim.xtc", "sim.pdb", "sim.top"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let errors = validate_meta(dir.path(), &minimal_meta(), None).unwrap();
        assert!(errors.iter().any(|e| e.contains("empty")));
    }

    /// Checking an upload against its manifest belongs to `ticket`, so adding
    /// one -- even a manifest every file contradicts -- changes nothing here.
    #[test]
    fn ignores_completed_json() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let meta = minimal_meta();
        let before = validate_meta(dir.path(), &meta, None).unwrap();

        let json = r#"{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{"irods_path":"sim.xtc","size":9999,"md5_hash":"00000000000000000000000000000000"}]}"#;
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();

        assert_eq!(validate_meta(dir.path(), &meta, None).unwrap(), before);
    }
}
