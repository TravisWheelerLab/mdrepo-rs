use crate::{
    common::{get_md5, read_file},
    constants::MAX_FILE_SIZE_BYTES,
    metadata::Meta,
    types::SubmissionCompleteJson,
};
use anyhow::{anyhow, bail, Result};
use log::info;
use std::{fs, path::PathBuf};

// --------------------------------------------------
pub fn validate(dir: &PathBuf) -> Result<()> {
    dbg!(&dir);

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
    dbg!(&files);

    if files.is_empty() {
        bail!(r#""{}" contains no files"#, dir.display());
    }
    info!("Found files: {files:#?}");

    let completed_path = dir.join("mdrepo-submission.completed.json");
    if !completed_path.is_file() {
        bail!(r#"Missing "{}""#, completed_path.display());
    }

    let completed: SubmissionCompleteJson =
        serde_json::from_str(&read_file(&completed_path)?).map_err(|e| {
            anyhow!(r#"Failed to parse "{}": {e}"#, completed_path.display())
        })?;
    dbg!(&completed);

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
    let mut total_file_size = 0;
    for file in &completed.files {
        let path = dir.join(&file.irods_path);
        if !path.exists() {
            bail!(r#"Missing expected file "{}""#, file.irods_path);
        }

        let size = path.metadata()?.len();
        total_file_size += size;

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

    // Check min/max file size
    if total_file_size == 0 {
        bail!("All local files are empty!");
    }

    if total_file_size > MAX_FILE_SIZE_BYTES {
        bail!(
            "Total file size ({total_file_size}) exceeds limit {MAX_FILE_SIZE_BYTES}"
        );
    }

    if total_file_size != completed.total_filesize {
        bail!(
            "Total file size ({total_file_size}) does not match meta {}",
            completed.total_filesize
        );
    }

    // Validate meta
    let meta_toml = dir.join("mdrepo-metadata.toml");
    if !meta_toml.is_file() {
        bail!("Missing {}", meta_toml.display());
    }

    // This automatically checks the validity of the TOML
    let meta = Meta::from_file(&meta_toml)?;
    dbg!(&meta);

    let uploaded_file_names: Vec<_> = completed
        .files
        .into_iter()
        .map(|file| file.irods_path.clone())
        .collect();

    for (file_type, file) in &[
        ("trajectory_file_name", meta.trajectory_file_name),
        ("structure_file_name", meta.structure_file_name),
        ("topology_file_name", meta.topology_file_name),
    ] {
        if !uploaded_file_names.contains(file) {
            bail!(r#"Metadata is missing "initial.{file_type}" file "{file}""#);
        }
    }

    println!("Validation complete");

    Ok(())
}
