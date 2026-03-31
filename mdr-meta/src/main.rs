use anyhow::{bail, Result};
use clap::Parser;
use libmdrepo::metadata::Meta;
use mdr_meta::{
    generate::generate,
    types::{Cli, Command, FileFormat},
};
use std::{
    fs::File,
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
            let meta = generate(args)?;
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
