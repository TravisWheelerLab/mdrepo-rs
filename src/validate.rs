use crate::{
    constants::MAX_FILE_SIZE_BYTES,
    types::{SubmissionCompleteJson, ValidateArgs},
};
use anyhow::{anyhow, bail, Result};
use chksum_md5 as md5;
use log::info;
use std::{
    fs::{self, File},
    path::Path,
};

// --------------------------------------------------
pub fn validate(args: &ValidateArgs) -> Result<()> {
    //dbg!(&args);

    info!(r#"Processing input directory "{}""#, args.dirname);
    let dir = Path::new(&args.dirname);

    let mut files: Vec<String> = fs::read_dir(&dir)?
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map_or(false, |t| t.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();

    if files.is_empty() {
        bail!(r#""{}" contains no files"#, dir.display());
    }
    info!("Found files: {files:#?}");

    // GetSubmissionCompletedDataStep
    let completed_path = dir.join("mdrepo-submission.completed.json");
    let completed: SubmissionCompleteJson =
        serde_json::from_str(&read_file(&completed_path.to_string_lossy())?)?;
    //dbg!(&completed);

    // Ensure that some files were uploaded
    if completed.files.is_empty() {
        bail!(r#""{}" contains no "files"#, completed_path.display());
    }

    // Check file hashes/sizes
    let mut total_file_size = 0;
    for file in completed.files {
        let path = dir.join(&file.irods_path);
        if !path.exists() {
            bail!(r#"Missing expected file "{}""#, file.irods_path);
        }

        let size = path.metadata()?.len();
        total_file_size += size;

        let fh = File::open(&path).map_err(|e| anyhow!("{}: {e}", &file.irods_path))?;
        let digest = md5::chksum(fh)?.to_hex_lowercase();

        info!(
            r#"Checking "{}" = hash {}, size {}"#,
            file.irods_path,
            if file.md5_hash == digest { "OK" } else { "BAD" },
            if file.size == size { "OK" } else { "Bad" }
        );

        if file.md5_hash != digest {
            bail!(
                r#""{}" MD5 "{}" does not match meta "{}""#,
                file.irods_path,
                digest,
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
    //let meta = Meta::from_file(&meta_toml)?;
    //dbg!(meta);

    //// CheckMetadataFilesExistStep
    //match meta.required_files {
    //    Some(files) => {
    //        println!("{files:?}");
    //        let uploaded_files: Vec<_> = completed
    //            .files
    //            .into_iter()
    //            .map(|file| file.irods_path)
    //            .collect();
    //        for (file_type, file) in &[
    //            ("Trajectory", files.trajectory_file_name),
    //            ("Structure", files.structure_file_name),
    //            ("Topology", files.topology_file_name),
    //        ] {
    //            if !uploaded_files.contains(file) {
    //                bail!(r#"Metadata "{file_type}" file "{file}" not uploaded"#);
    //            }
    //        }
    //    }

    //    _ => bail!("TOML data is missing required_files"),
    //}

    //// CheckTokenStep
    //check_token()?;

    // GetTokenStep
    // This step in python gets the "token" from the landing ID:
    // base64.urlsafe_b64encode(base64.b32decode(landing_id)).decode()
    // But what exactly is the "landing ID"? And why do we need this token?

    // ValidateMetadataStep
    // Some parts are validated by the types, e.g., required fields
    // but I may need to revisit this as I had to loosen some requirements
    // to parse older TOML formats. The validation also checks if a field
    // value must conform to allowed choices.

    println!("Validation complete");
    Ok(())
}

// --------------------------------------------------
fn read_file(filename: &str) -> Result<String> {
    fs::read_to_string(filename).map_err(|e| anyhow!("{filename}: {e}"))
}
