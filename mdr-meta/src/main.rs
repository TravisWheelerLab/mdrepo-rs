use anyhow::Result;
use clap::{builder::PossibleValue, Parser, ValueEnum};
use libmdrepo::metadata::Meta;
use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

// --------------------------------------------------
#[derive(Parser, Debug)]
#[command(arg_required_else_help = true, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

// --------------------------------------------------
#[derive(Parser, Debug)]
pub enum Command {
    /// Check TOML
    Check(CheckArgs),

    /// Generate a full example file
    Example(ExampleArgs),

    /// Print metadata in JSON format
    ToJson(ToJsonArgs),

    /// Print metadata in TOML format
    ToToml(ToTomlArgs),
}

#[derive(Debug, PartialEq, Clone)]
pub enum FileFormat {
    Json,
    Toml,
}

impl ValueEnum for FileFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[FileFormat::Json, FileFormat::Toml]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            FileFormat::Json => PossibleValue::new("json"),
            FileFormat::Toml => PossibleValue::new("toml"),
        })
    }
}

#[derive(Debug, Parser)]
pub struct ExampleArgs {
    /// Output format
    #[arg(
        short,
        long,
        value_name = "FORMAT",
        value_parser(clap::value_parser!(FileFormat)),
    )]
    format: Option<FileFormat>,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    outfile: String,
}

#[derive(Debug, Parser)]
pub struct ToJsonArgs {
    /// Input filename
    #[arg(value_name = "FILE")]
    filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    outfile: String,
}

#[derive(Debug, Parser)]
pub struct ToTomlArgs {
    /// Input filename
    #[arg(value_name = "FILE")]
    filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    outfile: String,
}

// --------------------------------------------------
#[derive(Parser, Debug)]
#[command(alias = "ch")]
pub struct CheckArgs {
    /// Input filename or "-" for STDIN
    #[arg(value_name = "FILE", num_args = 1..)]
    pub filenames: Vec<String>,
}

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("{e}");
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
        Some(Command::Example(args)) => {
            let mut out_file = open_outfile(&args.outfile)?;
            let format = args.format.clone().unwrap_or(guess_format(&args.outfile));
            let meta = Meta::example();
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
        out_name => Ok(Box::new(File::create(out_name)?)),
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
