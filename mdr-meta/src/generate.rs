use crate::types::{FileInfo, FileType, GenArgs};
use anyhow::{Result, bail};
use libmdrepo::{
    constants::{
        ENGINE_FILE_FORMATS, EngineFileFormats, STRUCTURE_FILE_EXTS,
        TOPOLOGY_FILE_EXTS, TRAJECTORY_FILE_EXTS, VALID_SOFTWARE,
    },
    metadata::{AdditionalFile, Creator, Meta},
};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

// --------------------------------------------------
pub fn generate(args: &GenArgs) -> Result<Meta> {
    if !VALID_SOFTWARE.contains_key(args.software.as_str()) {
        let valid_software_names: Vec<&str> = VALID_SOFTWARE.keys().copied().collect();
        bail!(
            r#"--software "{}" invalid, choose from {}"#,
            args.software,
            valid_software_names.join(", "),
        );
    }
    // CUSTOM has no entry in ENGINE_FILE_FORMATS -- it's the deliberate
    // escape hatch for pipelines with no fixed convention, so files are
    // classified against "recognized by any engine at all" instead of one
    // engine's own lists.
    let engine_formats = ENGINE_FILE_FORMATS.get(args.software.as_str());

    let dir = args
        .directory
        .clone()
        .map_or(env::current_dir()?, |val| PathBuf::from(&val));

    // main.rs's open_outfile() creates --outfile before calling generate(),
    // and refuses to run at all if that path already existed -- so by the
    // time this scan runs, a fresh, empty --outfile is guaranteed to be on
    // disk. If it's inside `dir`, exclude it: without this, gen would list
    // its own not-yet-written output as one of the directory's additional
    // files, which mdr-process's import only avoids duplicating on because
    // it dedupes original_files by md5 (see process.rs collect_original_files)
    // -- fine at import time, but wrong for `gen` to produce in the first
    // place. "-" means stdout; nothing on disk to exclude.
    let outfile_path = if args.outfile == "-" {
        None
    } else {
        fs::canonicalize(&args.outfile).ok()
    };

    let mut files = vec![];
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(outfile_path) = &outfile_path {
            if path.canonicalize()? == *outfile_path {
                continue;
            }
        }

        let file_name = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("No filename for '{}'", path.display()))?
            .to_string_lossy()
            .to_string();
        if file_name.starts_with(".") || file_name.starts_with("#") {
            continue;
        }

        let ext = path
            .extension()
            .map_or("".to_string(), |val| val.to_string_lossy().to_string());

        let file_type = classify_extension(&ext, engine_formats);
        let metadata = entry.metadata()?;
        files.push(FileInfo {
            file_name,
            file_type,
            size: metadata.len(),
        });
    }

    let trajectories = match &args.trajectory {
        Some(filename) => vec![filename.clone()],
        _ => files
            .iter()
            .filter(|f| f.file_type == FileType::Trajectory)
            .map(|f| f.file_name.clone())
            .collect(),
    };

    let structure =
        select_candidate(&args.structure, FileType::Structure, &files, &dir)?;

    let topology = select_candidate(&args.topology, FileType::Topology, &files, &dir)?;

    let mut taken: HashSet<&String> = trajectories.iter().collect();
    let _ = taken.insert(&structure);
    let _ = taken.insert(&topology);

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

    meta.trajectory_file_names = trajectories;

    meta.structure_file_name = structure;

    meta.topology_file_name = topology;

    meta.software_name = args.software.clone();

    meta.software_version = "<software_version> (required)".to_string();

    meta.creators = Some(vec![Creator {
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
/// Classifies a file extension as Trajectory/Structure/Topology/Other.
///
/// Given `engine_formats` (the declared engine's own row in
/// ENGINE_FILE_FORMATS), an extension is classified by which *one* of that
/// engine's three lists it appears in. Some extensions are genuinely
/// ambiguous even within a single engine -- GROMACS's `.tpr` is valid as
/// both structure and topology -- so an extension matching more than one
/// list is `Other` rather than a guess; the submitter must say via
/// `--structure`/`--topology`/`--trajectory`. Same for zero matches: an
/// extension this engine doesn't use at all is Other, not assigned anyway.
///
/// `engine_formats` is None only for CUSTOM (no fixed convention to check
/// against), in which case this falls back to "recognized by any engine at
/// all" -- the same union the check used before `--software` existed.
fn classify_extension(
    ext: &str,
    engine_formats: Option<&EngineFileFormats>,
) -> FileType {
    let Some(fmt) = engine_formats else {
        return if TRAJECTORY_FILE_EXTS.contains(&ext) {
            FileType::Trajectory
        } else if STRUCTURE_FILE_EXTS.contains(&ext) {
            FileType::Structure
        } else if TOPOLOGY_FILE_EXTS.contains(&ext) {
            FileType::Topology
        } else {
            FileType::Other
        };
    };

    let mut matched = [
        (fmt.trajectory.contains(&ext), FileType::Trajectory),
        (fmt.structure.contains(&ext), FileType::Structure),
        (fmt.topology.contains(&ext), FileType::Topology),
    ]
    .into_iter()
    .filter_map(|(is_match, role)| is_match.then_some(role));

    match (matched.next(), matched.next()) {
        (Some(role), None) => role,
        _ => FileType::Other,
    }
}

// --------------------------------------------------
fn select_candidate(
    provided_name: &Option<String>,
    file_type: FileType,
    files: &[FileInfo],
    directory: &Path,
) -> Result<String> {
    match provided_name {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn gen_args(software: &str, directory: Option<&Path>) -> GenArgs {
        GenArgs {
            software: software.to_string(),
            directory: directory.map(|d| d.to_string_lossy().to_string()),
            format: None,
            outfile: "-".to_string(),
            trajectory: None,
            structure: None,
            topology: None,
        }
    }

    // --- classify_extension: unambiguous within a known engine ---

    #[test]
    fn classify_gromacs_extensions() {
        let fmt = ENGINE_FILE_FORMATS.get("GROMACS");
        assert_eq!(classify_extension("xtc", fmt), FileType::Trajectory);
        assert_eq!(classify_extension("gro", fmt), FileType::Structure);
        assert_eq!(classify_extension("top", fmt), FileType::Topology);
    }

    // --- classify_extension: the same extension, different engines ---

    #[test]
    fn classify_crd_depends_on_engine() {
        // .crd is AMBER's ASCII trajectory format but CHARMM/NAMD's
        // structure format -- the same extension means different things
        // depending on which engine produced the file.
        assert_eq!(
            classify_extension("crd", ENGINE_FILE_FORMATS.get("AMBER")),
            FileType::Trajectory
        );
        assert_eq!(
            classify_extension("crd", ENGINE_FILE_FORMATS.get("CHARMM")),
            FileType::Structure
        );
    }

    #[test]
    fn classify_gro_depends_on_engine() {
        // .gro is GROMACS's structure file but SPONGE's topology file.
        assert_eq!(
            classify_extension("gro", ENGINE_FILE_FORMATS.get("GROMACS")),
            FileType::Structure
        );
        assert_eq!(
            classify_extension("gro", ENGINE_FILE_FORMATS.get("SPONGE")),
            FileType::Topology
        );
    }

    // --- classify_extension: ambiguous even within one engine ---

    #[test]
    fn classify_tpr_is_ambiguous_within_gromacs() {
        // GROMACS's .tpr is valid as both structure and topology -- don't
        // guess, the submitter has to say via --structure/--topology.
        assert_eq!(
            classify_extension("tpr", ENGINE_FILE_FORMATS.get("GROMACS")),
            FileType::Other
        );
    }

    #[test]
    fn classify_unrecognized_extension_for_engine_is_other() {
        // .psf isn't in AMBER's list at all (it's CHARMM/NAMD/ACEMD's).
        assert_eq!(
            classify_extension("psf", ENGINE_FILE_FORMATS.get("AMBER")),
            FileType::Other
        );
    }

    // --- classify_extension: CUSTOM falls back to the cross-engine union ---

    #[test]
    fn classify_custom_falls_back_to_union() {
        assert!(ENGINE_FILE_FORMATS.get("CUSTOM").is_none());
        assert_eq!(classify_extension("xtc", None), FileType::Trajectory);
        assert_eq!(classify_extension("not_a_real_ext", None), FileType::Other);
    }

    // --- generate(): --software validation ---

    #[test]
    fn generate_rejects_invalid_software() {
        let args = gen_args("NOTAREALENGINE", None);
        let err = generate(&args).unwrap_err();
        assert!(err.to_string().contains("NOTAREALENGINE"));
        assert!(err.to_string().contains("choose from"));
    }

    // --- generate(): end-to-end classification with a real directory ---

    #[test]
    fn generate_classifies_gromacs_directory() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("sim.gro"), b"structure").unwrap();
        fs::write(dir.path().join("sim.top"), b"topology").unwrap();
        fs::write(dir.path().join("sim.xtc"), b"trajectory").unwrap();

        let args = gen_args("GROMACS", Some(dir.path()));
        let meta = generate(&args).unwrap();
        assert_eq!(meta.software_name, "GROMACS");
        assert_eq!(meta.structure_file_name, "sim.gro");
        assert_eq!(meta.topology_file_name, "sim.top");
        assert_eq!(meta.trajectory_file_names, vec!["sim.xtc".to_string()]);
    }

    #[test]
    fn generate_excludes_its_own_outfile_from_additional_files() {
        // main.rs's open_outfile() creates --outfile before calling
        // generate() -- simulate that here rather than going through
        // main.rs, since this is a unit test of generate() alone. Without
        // the exclusion, writing into the same directory being scanned
        // would list the not-yet-written output file as one of its own
        // additional files.
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("sim.gro"), b"structure").unwrap();
        fs::write(dir.path().join("sim.top"), b"topology").unwrap();
        fs::write(dir.path().join("sim.xtc"), b"trajectory").unwrap();
        fs::write(dir.path().join("README.txt"), b"notes").unwrap();

        let outfile = dir.path().join("mdrepo-metadata.toml");
        fs::write(&outfile, b"").unwrap();

        let mut args = gen_args("GROMACS", Some(dir.path()));
        args.outfile = outfile.to_string_lossy().to_string();

        let meta = generate(&args).unwrap();
        let names: Vec<&str> = meta
            .additional_files
            .as_ref()
            .unwrap()
            .iter()
            .map(|f| f.file_name.as_str())
            .collect();
        assert_eq!(names, vec!["README.txt"]);
    }

    #[test]
    fn generate_leaves_ambiguous_tpr_unassigned() {
        // A lone .tpr under GROMACS can't be auto-assigned to either
        // structure or topology (classify_extension returns Other for it),
        // so both fall back to the "required" placeholder instead of one
        // silently winning.
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("run.tpr"), b"topology-or-structure").unwrap();
        fs::write(dir.path().join("sim.xtc"), b"trajectory").unwrap();

        let args = gen_args("GROMACS", Some(dir.path()));
        let meta = generate(&args).unwrap();
        assert!(meta.structure_file_name.contains("required"));
        assert!(meta.topology_file_name.contains("required"));
    }
}
