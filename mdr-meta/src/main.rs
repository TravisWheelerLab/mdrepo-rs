use anyhow::{Result, bail};
use clap::Parser;
use libmdrepo::metadata::{Meta, MetaCheckOptions};
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
// Exit codes, following the convention grep uses:
//   0 = OK
//   1 = the metadata is invalid (the errors are on stdout)
//   2 = this program could not do its job
// Callers must be able to tell "your TOML is bad" from "the check never
// ran," so do not collapse 1 and 2 into a single failure code.
fn main() {
    match run(Cli::parse()) {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(2);
        }
    }
}

// --------------------------------------------------
/// Returns whether the input passed. Only `check` can return false:
/// invalid metadata is a result to report, not an error to raise.
fn run(args: Cli) -> Result<bool> {
    match &args.command {
        Some(Command::Check(args)) => {
            let num_files = args.filenames.len();
            let mut num_invalid = 0;

            for filename in &args.filenames {
                if num_files > 1 {
                    println!("==> {filename} <==")
                }

                let opts = args.no_id.then_some(MetaCheckOptions {
                    allow_no_pdb_uniprot: true,
                    ..Default::default()
                });

                // A file that will not parse is invalid metadata, so it
                // is reported like any other finding rather than aborting
                // the remaining files.
                let errors = match parse_file(filename) {
                    Ok(meta) => meta.check(opts),
                    Err(e) => vec![e.to_string()],
                };

                if !errors.is_empty() {
                    num_invalid += 1;
                    println!("{}", errors.join("\n"));
                }
            }

            return Ok(num_invalid == 0);
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
        Some(Command::Upgrade) => {
            let status = self_update::backends::github::Update::configure()
                .repo_owner("TravisWheelerLab")
                .repo_name("mdrepo-rs")
                .bin_name("mdr-meta")
                .show_download_progress(true)
                .current_version(self_update::cargo_crate_version!())
                .build()?
                .update()?;
            println!("Update status: `{}`!", status.version());
        }
        _ => unreachable!(),
    }

    Ok(true)
}

// --------------------------------------------------
fn parse_file(filename: &str) -> Result<Meta> {
    match filename {
        "-" => {
            let mut lines = vec![];
            for line in io::stdin().lines() {
                lines.push(line?);
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
