use clap::{builder::PossibleValue, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// --------------------------------------------------
#[derive(Parser, Debug)]
#[command(arg_required_else_help = true, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Log level
    #[arg(short, long)]
    pub log: Option<LogLevel>,

    /// Log file
    #[arg(long)]
    pub log_file: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Debug,
}

impl ValueEnum for LogLevel {
    fn value_variants<'a>() -> &'a [Self] {
        &[LogLevel::Info, LogLevel::Debug]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            LogLevel::Info => PossibleValue::new("info"),
            LogLevel::Debug => PossibleValue::new("debug"),
        })
    }
}

// --------------------------------------------------
#[derive(Parser, Debug)]
pub enum Command {
    /// Validate simulation directory
    Validate(ValidateArgs),

    /// Check metadata
    MetaCheck(MetaCheckArgs),

    /// Process a simulation
    Process(ProcessArgs),
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "pr")]
pub struct ProcessArgs {
    /// Input directory
    #[arg(value_name = "DIR")]
    pub dirname: PathBuf,

    /// Script directory
    #[arg(
        short,
        long,
        value_name = "DIR",
        default_value = "/opt/mdrepo/simulation-processing/python/"
    )]
    pub script_dir: Option<PathBuf>,

    /// Output directory for processed files
    #[arg(short, long, value_name = "OUTDIR")]
    pub outdir: Option<PathBuf>,

    /// Output directory for JSON import file
    #[arg(short, long, value_name = "OUTDIR", default_value = "import_json")]
    pub json_dir: Option<PathBuf>,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "va")]
pub struct ValidateArgs {
    /// Input directory
    #[arg(value_name = "DIR")]
    pub dirname: PathBuf,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "ch")]
pub struct MetaCheckArgs {
    /// Input file
    #[arg(value_name = "FILE")]
    pub filename: PathBuf,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionCompleteJson {
    pub total_filenum: u32,
    pub total_filesize: u64,
    pub token: Option<String>,
    pub status: String,
    pub files: Vec<SubmissionCompleteFile>,
    pub time: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionCompleteFile {
    pub irods_path: String,
    pub size: u64,
    pub md5_hash: String,
}

// --------------------------------------------------
pub struct FullMinFiles {
    pub meta_toml: PathBuf,
    pub full_gro: PathBuf,
    pub full_pdb: PathBuf,
    pub full_xtc: PathBuf,
    pub min_gro: PathBuf,
    pub min_pdb: PathBuf,
    pub min_xtc: PathBuf,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProteinSequence {
    pub three_letters: String,
    pub single_letters: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RmsdRmsf {
    pub rmsd: Vec<f64>,
    pub rmsf: Vec<f64>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Duration {
    pub totaltime_ns: u32,
    pub sampling_frequency_ns: f32,
}

// --------------------------------------------------
#[derive(Debug)]
pub struct UniprotEntry {
    pub uniprot_id: String,
    pub name: String,
    pub sequence: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct UniprotResponse {
    #[serde(alias = "proteinDescription")]
    pub protein_description: UniprotProteinDesc,

    pub sequence: UniprotProteinSequence,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct UniprotProteinDesc {
    #[serde(alias = "recommendedName")]
    pub recommended_name: Option<UniprotProteinFullName>,

    #[serde(alias = "submissionNames")]
    pub submission_names: Option<UniprotProteinFullName>,
}
// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct UniprotProteinFullName {
    #[serde(alias = "fullName")]
    pub full_name: UniprotProteinFullNameValue,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct UniprotProteinFullNameValue {
    pub value: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct UniprotProteinSequence {
    pub value: String,
}

// --------------------------------------------------
#[derive(Debug)]
pub struct PdbEntry {
    pub pdb_id: String,
    pub title: String,
    pub classification: String,
    pub uniprots: Vec<UniprotEntry>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbResponse {
    #[serde(alias = "struct")]
    pub struct_: PdbStruct,

    pub struct_keywords: PdbStructKeywords,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbStruct {
    pub title: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbStructKeywords {
    pub pdbx_keywords: String,
}
