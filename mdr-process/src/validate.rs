use anyhow::{bail, Result};
use libmdrepo::metadata::{Meta, MetaCheckOptions};
use log::debug;
use std::{fs, path::Path};

// --------------------------------------------------
/// Check that a directory holds a processable simulation: the metadata parses,
/// passes its own checks, and the files it references are present and not all
/// empty. This says nothing about how the directory got here -- verifying an
/// upload against its manifest is `ticket::check_manifest`.
pub fn validate(
    dir: &Path,
    meta_check_opts: Option<MetaCheckOptions>,
) -> Result<Vec<String>> {
    if !dir.is_dir() {
        bail!(r#"Invalid directory "{}""#, dir.display())
    }

    debug!(r#"Processing input directory "{}""#, dir.display());

    let mut files: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();

    let num_files = files.len();
    debug!(
        "Found {num_files} file{}",
        if num_files == 1 { "" } else { "s" }
    );

    if num_files == 0 {
        bail!(r#""{}" is empty!"#, dir.display());
    }

    // Validate meta
    let meta_path = dir.join("mdrepo-metadata.toml");
    if !meta_path.is_file() {
        bail!("Missing {}", meta_path.display());
    }
    let meta = Meta::from_file(&meta_path)?;
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

    fn write_minimal_dir(dir: &Path) {
        let toml = r#"
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
        fs::write(dir.join("mdrepo-metadata.toml"), toml).unwrap();
        fs::write(dir.join("sim.xtc"), b"trajectory").unwrap();
        fs::write(dir.join("sim.pdb"), b"structure").unwrap();
        fs::write(dir.join("sim.top"), b"topology").unwrap();
    }

    #[test]
    fn rejects_nonexistent_dir() {
        assert!(validate(Path::new("/nonexistent/path"), None).is_err());
    }

    #[test]
    fn rejects_empty_dir() {
        let dir = tempdir().unwrap();
        let err = validate(dir.path(), None).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn rejects_dir_without_toml() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("sim.xtc"), b"data").unwrap();
        let err = validate(dir.path(), None).unwrap_err();
        assert!(err.to_string().contains("Missing"));
    }

    #[test]
    fn rejects_invalid_toml() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("mdrepo-metadata.toml"), "not {{ valid").unwrap();
        assert!(validate(dir.path(), None).is_err());
    }

    #[test]
    fn accepts_valid_submission() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        assert!(validate(dir.path(), None).is_ok());
    }

    #[test]
    fn reports_file_referenced_by_metadata_but_missing() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        fs::remove_file(dir.path().join("sim.xtc")).unwrap();
        let errors = validate(dir.path(), None).unwrap();
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
        let errors = validate(dir.path(), None).unwrap();
        assert!(errors.iter().any(|e| e.contains("empty")));
    }

    /// Checking an upload against its manifest belongs to `ticket`, so adding
    /// one -- even a manifest every file contradicts -- changes nothing here.
    #[test]
    fn ignores_completed_json() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let before = validate(dir.path(), None).unwrap();

        let json = r#"{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{"irods_path":"sim.xtc","size":9999,"md5_hash":"00000000000000000000000000000000"}]}"#;
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();

        assert_eq!(validate(dir.path(), None).unwrap(), before);
    }
}
