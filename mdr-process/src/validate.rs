use crate::types::SubmissionCompleteJson;
use anyhow::{anyhow, bail, Result};
use libmdrepo::{
    common::{get_md5, read_file},
    constants::MAX_FILE_SIZE_BYTES,
    metadata::{Meta, MetaCheckOptions},
};
use log::debug;
use std::{collections::HashMap, fs, path::Path};

// --------------------------------------------------
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

    let mut errors = vec![];
    let mut md5_hashes: HashMap<String, Vec<String>> = HashMap::new();
    let completed_path = dir.join("mdrepo-submission.completed.json");
    if completed_path.is_file() {
        let completed: SubmissionCompleteJson =
            serde_json::from_str(&read_file(&completed_path)?).map_err(|e| {
                anyhow!(r#"Failed to parse "{}": {e}"#, completed_path.display())
            })?;

        if completed.total_filenum as usize != completed.files.len() {
            bail!(
                "Expected {} file(s) but completed JSON has {}",
                completed.total_filenum,
                completed.files.len()
            );
        }

        // Ensure that some files were uploaded
        if completed.files.is_empty() {
            bail!(r#""{}" contains no "files"#, completed_path.display());
        }

        // Check file hashes/sizes
        for file in &completed.files {
            let path = dir.join(&file.irods_path);
            if !path.exists() {
                errors.push(format!(r#"Missing expected file "{}""#, file.irods_path));
                continue;
            }

            let size = path.metadata()?.len();
            let local_md5 = get_md5(&path)?;

            debug!(
                r#"Checking "{}" = expected md5 {}, size {}"#,
                file.irods_path,
                if file.md5_hash == local_md5 {
                    "OK"
                } else {
                    "BAD"
                },
                if file.size == size { "OK" } else { "Bad" }
            );

            // What if the size is 0 but so is the expected size in the completed JSON?
            //if size == 0 {
            //    errors.push(format!(r#""{}" is empty"#, file.irods_path));
            //}

            if file.md5_hash != local_md5 {
                errors.push(format!(
                    r#""{}" MD5 "{}" does not match meta "{}""#,
                    file.irods_path, local_md5, file.md5_hash
                ));
            }

            if file.size != size {
                errors.push(format!(
                    r#""{}" size "{}" does not match meta "{}""#,
                    file.irods_path, size, file.size
                ))
            }
            md5_hashes
                .entry(local_md5)
                .or_default()
                .push(file.irods_path.clone());
        }
    }

    for (hash, files) in md5_hashes {
        if files.len() > 1 {
            errors.push(format!(
                r#"File hash "{hash}" is duplicated: [{}]"#,
                files.join(", ")
            ))
        }
    }

    // Validate meta
    let meta_path = dir.join("mdrepo-metadata.toml");
    if !meta_path.is_file() {
        bail!("Missing {}", meta_path.display());
    }
    let meta = Meta::from_file(&meta_path)?;
    let meta_errors = meta.check(meta_check_opts);
    errors.extend(meta_errors);

    let mut total_file_size = 0;
    for filename in &meta.all_filenames() {
        let metadata = dir
            .join(filename)
            .metadata()
            .map_err(|e| anyhow!("{filename}: {e}"))?;

        total_file_size += metadata.len();
    }

    // Check min/max file size
    if total_file_size == 0 {
        errors.push("All local files are empty!".to_string());
    }

    if total_file_size > MAX_FILE_SIZE_BYTES {
        errors.push(format!(
            "Total file size ({total_file_size}) exceeds limit {MAX_FILE_SIZE_BYTES}"
        ));
    }

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libmdrepo::common::get_md5;
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
    fn completed_json_wrong_filenum_bails() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let json = r#"{"total_filenum":5,"total_filesize":0,"status":"completed","files":[{"irods_path":"sim.xtc","size":9,"md5_hash":"abc"}]}"#;
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();
        assert!(validate(dir.path(), None).is_err());
    }

    #[test]
    fn completed_json_missing_file_returns_error() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let json = r#"{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{"irods_path":"ghost.xtc","size":0,"md5_hash":"abc123abc123abc123abc123abc12300"}]}"#;
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();
        let errors = validate(dir.path(), None).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing") && e.contains("ghost.xtc")));
    }

    #[test]
    fn completed_json_md5_mismatch_returns_error() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let json = r#"{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{"irods_path":"sim.xtc","size":9,"md5_hash":"00000000000000000000000000000000"}]}"#;
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();
        let errors = validate(dir.path(), None).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("MD5") && e.contains("sim.xtc")));
    }

    #[test]
    fn completed_json_size_mismatch_returns_error() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let xtc_md5 = get_md5(&dir.path().join("sim.xtc")).unwrap();
        let json = format!(
            r#"{{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{{"irods_path":"sim.xtc","size":9999,"md5_hash":"{xtc_md5}"}}]}}"#
        );
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();
        let errors = validate(dir.path(), None).unwrap();
        assert!(errors
            .iter()
            .any(|e| e.contains("size") && e.contains("sim.xtc")));
    }

    #[test]
    fn completed_json_duplicate_md5_returns_error() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        fs::write(dir.path().join("copy_a.dat"), b"identical").unwrap();
        fs::write(dir.path().join("copy_b.dat"), b"identical").unwrap();
        let md5 = get_md5(&dir.path().join("copy_a.dat")).unwrap();
        let json = format!(
            r#"{{"total_filenum":2,"total_filesize":0,"status":"completed","files":[
                {{"irods_path":"copy_a.dat","size":9,"md5_hash":"{md5}"}},
                {{"irods_path":"copy_b.dat","size":9,"md5_hash":"{md5}"}}
            ]}}"#
        );
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();
        let errors = validate(dir.path(), None).unwrap();
        assert!(errors.iter().any(|e| e.contains("duplicated")));
    }
}
