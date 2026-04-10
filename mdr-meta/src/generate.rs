use crate::types::{FileInfo, FileType, GenArgs};
use anyhow::{bail, Result};
use libmdrepo::{
    constants::{STRUCTURE_FILE_EXTS, TOPOLOGY_FILE_EXTS, TRAJECTORY_FILE_EXTS},
    metadata::{AdditionalFile, Contributor, Meta},
};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

// --------------------------------------------------
pub fn generate(args: &GenArgs) -> Result<Meta> {
    let dir = args
        .directory
        .clone()
        .map_or(env::current_dir()?, |val| PathBuf::from(&val));

    let mut files = vec![];
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        if file_name.starts_with(".") || file_name.starts_with("#") {
            continue;
        }

        let ext = path
            .extension()
            .map_or("".to_string(), |val| val.to_string_lossy().to_string());

        let file_type = if TRAJECTORY_FILE_EXTS.contains(&ext.as_str()) {
            FileType::Trajectory
        } else if STRUCTURE_FILE_EXTS.contains(&ext.as_str()) {
            FileType::Structure
        } else if TOPOLOGY_FILE_EXTS.contains(&ext.as_str()) {
            FileType::Topology
        } else {
            FileType::Other
        };
        let metadata = entry.metadata()?;
        files.push(FileInfo {
            file_name,
            file_type,
            size: metadata.len(),
        });
    }

    //let tmpl = args
    //    .template
    //    .clone()
    //    .map(|path| Meta::from_file(&path))
    //    .transpose()?;

    let trajectory =
        select_candidate(&args.trajectory, FileType::Trajectory, &files, &dir)?;

    let structure =
        select_candidate(&args.structure, FileType::Structure, &files, &dir)?;

    let topology = select_candidate(&args.topology, FileType::Topology, &files, &dir)?;

    let taken = HashSet::from([&trajectory, &structure, &topology]);

    let mut additional_files: Vec<_> = files
        .iter()
        .filter(|f| !taken.contains(&f.file_name))
        .map(|f| AdditionalFile {
            file_name: f.file_name.to_string(),
            file_type: f.file_type.to_string(),
            description: None,
        })
        .collect();
    additional_files.sort_by(|a, b| a.file_name.cmp(&b.file_name));

    let mut meta = Meta::example_minimal();

    //if let Some(filename) = &args.template {
    //    let tmpl = Meta::from_file(filename)?;
    //    meta.lead_contributor_orcid = tmpl.lead_contributor_orcid.to_string();
    //}

    meta.trajectory_file_name = trajectory;

    meta.structure_file_name = structure;

    meta.topology_file_name = topology;

    meta.software_name = "<software_name> (required)".to_string();

    meta.software_version = "<software_version> (required)".to_string();

    meta.contributors = Some(vec![Contributor {
        name: "<Your Name>".to_string(),
        institution: Some("<institution> (optional)".to_string()),
        email: Some("<email> (optional)".to_string()),
        orcid: Some("<orcid> (optional)".to_string()),
    }]);

    if !additional_files.is_empty() {
        meta.additional_files = Some(additional_files);
    };

    Ok(meta)
}

// --------------------------------------------------
fn select_candidate(
    wanted_name: &Option<String>,
    file_type: FileType,
    files: &[FileInfo],
    directory: &Path,
) -> Result<String> {
    match wanted_name {
        Some(name) => {
            let path = directory.join(name);
            if path.is_file() {
                Ok(name.to_string())
            } else {
                bail!(r#"{file_type} file "{name}" does not exist"#);
            }
        }
        _ => {
            let mut candidates: Vec<_> =
                files.iter().filter(|f| f.file_type == file_type).collect();

            // To select the largest
            candidates.sort_by_key(|f| f.size);

            if let Some(file) = candidates.last() {
                Ok(file.file_name.to_string())
            } else {
                Ok(format!(
                    "<{}_file_name> (required)",
                    file_type.to_string().to_lowercase()
                ))
            }
        }
    }
}
