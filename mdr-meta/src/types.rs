use clap::{builder::PossibleValue, Parser, ValueEnum};
use std::path::PathBuf;

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

    /// Create a full example file
    Eg(EgArgs),

    /// Generate metadata file from directory contents
    Gen(GenArgs),

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
pub struct EgArgs {
    /// Output format
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<FileFormat>,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,

    /// Only output required fields
    #[arg(short, long)]
    pub minimal: bool,
}

#[derive(Debug, Parser)]
pub struct GenArgs {
    /// Output format
    #[arg(short, long, value_name = "DIR")]
    pub directory: Option<String>,

    /// Output format
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<FileFormat>,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,

    /// Trajectory filename
    #[arg(long, value_name = "TRAJ")]
    pub trajectory: Option<String>,

    /// Structure filename
    #[arg(long, value_name = "STRUCT")]
    pub structure: Option<String>,

    /// Topology filename
    #[arg(long, value_name = "TOPO")]
    pub topology: Option<String>,

    /// TOML template
    #[arg(long, value_name = "TMPL")]
    pub template: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub struct ToJsonArgs {
    /// Input filename
    #[arg(value_name = "FILE")]
    pub filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,
}

#[derive(Debug, Parser)]
pub struct ToTomlArgs {
    /// Input filename
    #[arg(value_name = "FILE")]
    pub filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,
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
#[derive(strum_macros::Display, PartialEq)]
pub enum FileType {
    Trajectory,
    Structure,
    Topology,
    Other,
}

// --------------------------------------------------
pub struct FileInfo {
    pub file_name: String,
    pub file_type: FileType,
    pub size: u64,
}
