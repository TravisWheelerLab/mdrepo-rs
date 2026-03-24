use anyhow::{bail, Result};
use clap::Parser;
use libmdrepo::{
    constants::{STRUCTURE_FILE_EXTS, TOPOLOGY_FILE_EXTS, TRAJECTORY_FILE_EXTS},
    metadata::{AdditionalFile, Contributor, Meta},
};
use mdr_meta::types::{Cli, Command, FileFormat, FileInfo, FileType, GenArgs};
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Cli) -> Result<()> {
    match &args.command {
        Some(Command::Check(args)) => {
            let num_files = args.filenames.len();
            for filename in &args.filenames {
                if num_files > 1 {
                    println!("==> {filename} <==")
                }
                match parse_file(filename) {
                    Ok(meta) => println!("{}", meta.check().join("\n")),
                    Err(e) => println!("{e}"),
                }
            }
            ()
        }
        Some(Command::Eg(args)) => {
            let mut out_file = open_outfile(&args.outfile)?;
            let format = args.format.clone().unwrap_or(guess_format(&args.outfile));
            let meta = if args.minimal {
                Meta::example_minimal()
            } else {
                Meta::example()
            };
            write!(
                out_file,
                "{}",
                if format == FileFormat::Json {
                    meta.to_json()?
                } else {
                    meta.to_toml()?
                }
            )?;
        }
        Some(Command::Gen(args)) => {
            let mut out_file = open_outfile(&args.outfile)?;
            let format = args.format.clone().unwrap_or(guess_format(&args.outfile));
            let meta = meta_from_dir(args)?;
            write!(
                out_file,
                "{}",
                if format == FileFormat::Json {
                    meta.to_json()?
                } else {
                    meta.to_toml()?
                }
            )?;
        }
        Some(Command::ToJson(args)) => {
            let mut out_file = open_outfile(&args.outfile)?;
            let meta = parse_file(&args.filename)?;
            write!(out_file, "{}", meta.to_json()?)?;
        }
        Some(Command::ToToml(args)) => {
            let mut out_file = open_outfile(&args.outfile)?;
            let meta = parse_file(&args.filename)?;
            write!(out_file, "{}", meta.to_toml()?)?;
        }
        _ => unreachable!(),
    }

    Ok(())
}

// --------------------------------------------------
fn parse_file(filename: &str) -> Result<Meta> {
    match filename {
        "-" => {
            let mut lines = vec![];
            for line in io::stdin().lines() {
                lines.push(line.unwrap());
            }
            let contents = lines.join("\n");

            if let Ok(res) = Meta::from_toml(&contents) {
                Ok(res)
            } else {
                Meta::from_json(&contents)
            }
        }
        _ => Meta::from_file(&PathBuf::from(filename)),
    }
}

// --------------------------------------------------
fn open_outfile(filename: &str) -> Result<Box<dyn Write>> {
    match filename {
        "-" => Ok(Box::new(io::stdout())),
        out_name => {
            if Path::new(out_name).exists() {
                bail!(r#"--outfile "{filename}" already exists"#);
            } else {
                Ok(Box::new(File::create(out_name)?))
            }
        }
    }
}

// --------------------------------------------------
fn guess_format(filename: &str) -> FileFormat {
    if filename == "-" {
        FileFormat::Toml
    } else {
        match Path::new(filename).extension() {
            Some(ext) => {
                if ext == "json" {
                    FileFormat::Json
                } else {
                    FileFormat::Toml
                }
            }
            _ => FileFormat::Toml,
        }
    }
}

// --------------------------------------------------
fn meta_from_dir(args: &GenArgs) -> Result<Meta> {
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
    additional_files.sort_by_key(|f| f.file_name.to_string());

    let mut meta = Meta::example_minimal();

    if let Some(orcid) = &args.orcid {
        meta.lead_contributor_orcid = orcid.to_string();
    }

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

    if additional_files.is_empty() {
        meta.additional_files = Some(additional_files);
    };

    Ok(meta)
}

// --------------------------------------------------
fn select_candidate(
    wanted_name: &Option<String>,
    file_type: FileType,
    files: &Vec<FileInfo>,
    directory: &Path,
) -> Result<String> {
    match wanted_name {
        Some(name) => {
            let path = directory.join(&name);
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
