use anyhow::{bail, Result};
use clap::Parser;
use libmdrepo::{
    constants::{STRUCTURE_FILE_EXTS, TOPOLOGY_FILE_EXTS, TRAJECTORY_FILE_EXTS},
    metadata::{AdditionalFile, Contributor, Meta},
};
use mdr_meta::types::{Cli, Command, FileFormat};
use std::{
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
            let meta = meta_from_dir(args.directory.clone())?;
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
fn meta_from_dir(dir: Option<String>) -> Result<Meta> {
    let dir = dir.map_or(env::current_dir()?, |val| PathBuf::from(&val));
    let mut trajectory: Option<String> = None;
    let mut structure: Option<String> = None;
    let mut topology: Option<String> = None;
    let mut additional_files = vec![];

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        match path.extension() {
            Some(ext) => {
                let ext = ext.to_string_lossy().to_string();
                if TRAJECTORY_FILE_EXTS.contains(&ext.as_str()) {
                    trajectory = Some(file_name);
                } else if STRUCTURE_FILE_EXTS.contains(&ext.as_str()) {
                    structure = Some(file_name);
                } else if TOPOLOGY_FILE_EXTS.contains(&ext.as_str()) {
                    topology = Some(file_name);
                } else {
                    additional_files.push(file_name);
                }
            }
            _ => additional_files.push(file_name),
        }
    }
    additional_files.sort();

    let mut meta = Meta::example_minimal();

    meta.trajectory_file_name =
        trajectory.unwrap_or("<trajectory> (required)".to_string());

    meta.structure_file_name =
        structure.unwrap_or("<structure> (required)".to_string());

    meta.topology_file_name = topology.unwrap_or("<topology> (required)".to_string());

    meta.software_name = "<software_name> (required)".to_string();

    meta.software_version = "<software_version> (required)".to_string();

    meta.contributors = Some(vec![Contributor {
        name: "<Your Name>".to_string(),
        institution: Some("<institution> (optional)".to_string()),
        email: Some("<email> (optional)".to_string()),
        orcid: Some("<orcid> (optional)".to_string()),
    }]);

    if !additional_files.is_empty() {
        meta.additional_files = Some(
            additional_files
                .iter()
                .map(|name| AdditionalFile {
                    file_name: name.to_string(),
                    file_type: "<file_type> (required)".to_string(),
                    description: Some("<description> (optional)".to_string()),
                })
                .collect::<Vec<_>>(),
        )
    };

    Ok(meta)
}
