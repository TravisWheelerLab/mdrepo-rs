use crate::{constants::MAX_FILE_SIZE_BYTES, types::SubmissionCompleteJson, Meta};
use anyhow::{anyhow, bail, Result};
//use chksum_md5 as md5;
use log::info;
use std::{
    //fs::{self, File},
    fs,
    path::PathBuf,
};

// --------------------------------------------------
pub fn validate(dir: &PathBuf) -> Result<()> {
    dbg!(&dir);

    if !dir.is_dir() {
        bail!(r#"Invalid directory "{}""#, dir.display())
    }

    info!(r#"Processing input directory "{}""#, dir.display());

    let mut files: Vec<String> = fs::read_dir(&dir)?
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map_or(false, |t| t.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    dbg!(&files);

    if files.is_empty() {
        bail!(r#""{}" contains no files"#, dir.display());
    }
    info!("Found files: {files:#?}");

    // GetSubmissionCompletedDataStep
    let completed_path = dir.join("mdrepo-submission.completed.json");
    if !completed_path.is_file() {
        bail!(r#"Missing "{}""#, completed_path.display());
    }

    let completed: SubmissionCompleteJson = serde_json::from_str(&read_file(
        &completed_path.to_string_lossy(),
    )?)
    .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, completed_path.display()))?;
    dbg!(&completed);

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

        //let fh = File::open(&path).map_err(|e| anyhow!("{}: {e}", &file.irods_path))?;
        //let digest = md5::chksum(fh)?.to_hex_lowercase();

        info!(
            r#"Checking "{}" = size {}"#,
            //r#"Checking "{}" = hash {}, size {}"#,
            file.irods_path,
            //if file.md5_hash == digest { "OK" } else { "BAD" },
            if file.size == size { "OK" } else { "Bad" }
        );

        //if file.md5_hash != digest {
        //    bail!(
        //        r#""{}" MD5 "{}" does not match meta "{}""#,
        //        file.irods_path,
        //        digest,
        //        file.md5_hash
        //    )
        //}

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

    // CheckMetadataFilesExistStep
    match meta.required_files {
        Some(reqd_file) => {
            println!("{reqd_file:?}");
            let uploaded_files: Vec<_> = completed
                .files
                .into_iter()
                .map(|file| file.irods_path.clone())
                .collect();
            for (file_type, file) in &[
                ("Trajectory", reqd_file.trajectory_file_name),
                ("Structure", reqd_file.structure_file_name),
                ("Topology", reqd_file.topology_file_name),
            ] {
                if !uploaded_files.contains(file) {
                    bail!(r#"Missing "{file_type}" file "{file}""#);
                }
            }
        }

        _ => bail!("TOML data is missing required_files"),
    }
    println!("Validation complete");

    Ok(())
}

// --------------------------------------------------
fn read_file(filename: &str) -> Result<String> {
    fs::read_to_string(filename).map_err(|e| anyhow!("{filename}: {e}"))
}
