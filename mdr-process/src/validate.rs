use crate::types::SubmissionCompleteJson;
use anyhow::{anyhow, bail, Result};
use libmdrepo::{
    common::{get_md5, read_file},
    constants::MAX_FILE_SIZE_BYTES,
    metadata::Meta,
};
use log::info;
use std::{fs, path::PathBuf};

// --------------------------------------------------
pub fn validate(dir: &PathBuf) -> Result<()> {
    if !dir.is_dir() {
        bail!(r#"Invalid directory "{}""#, dir.display())
    }

    info!(r#"Processing input directory "{}""#, dir.display());

    let mut files: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();

    let num_files = files.len();
    info!(
        "Found {num_files} file{}",
        if num_files == 1 { "" } else { "s" }
    );
    if num_files == 0 {
        bail!(r#""{}" is empty!"#, dir.display());
    }

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
                bail!(r#"Missing expected file "{}""#, file.irods_path);
            }

            let size = path.metadata()?.len();

            let local_md5 = get_md5(&path)?;

            info!(
                r#"Checking "{}" = md5 {}, size {}"#,
                file.irods_path,
                if file.md5_hash == local_md5 {
                    "OK"
                } else {
                    "BAD"
                },
                if file.size == size { "OK" } else { "Bad" }
            );

            if file.md5_hash != local_md5 {
                bail!(
                    r#""{}" MD5 "{}" does not match meta "{}""#,
                    file.irods_path,
                    local_md5,
                    file.md5_hash
                )
            }

            if file.size != size {
                bail!(
                    r#""{}" size "{}" does not match meta "{}""#,
                    file.irods_path,
                    size,
                    file.size
                )
            }
        }
    }

    // Validate meta
    let meta_toml = dir.join("mdrepo-metadata.toml");
    if !meta_toml.is_file() {
        bail!("Missing {}", meta_toml.display());
    }

    // This automatically checks the validity of the TOML
    let meta = Meta::from_file(&meta_toml)?;

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
        bail!("All local files are empty!");
    }

    if total_file_size > MAX_FILE_SIZE_BYTES {
        bail!(
            "Total file size ({total_file_size}) exceeds limit {MAX_FILE_SIZE_BYTES}"
        );
    }

    println!("Validation complete");

    Ok(())
}
