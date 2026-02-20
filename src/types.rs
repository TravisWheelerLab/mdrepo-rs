use clap::{builder::PossibleValue, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

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

    /// Process a new simulation upload
    Process(ProcessArgs),

    /// Reprocess an existing simulation
    Reprocess(ReprocessArgs),
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "pr")]
pub struct ProcessArgs {
    /// Input directory
    #[arg(value_name = "IN_DIR")]
    pub dirname: PathBuf,

    /// Script directory
    #[arg(short('S'), long, value_name = "SCRIPTS")]
    pub script_dir: Option<PathBuf>,

    /// Output directory for processed files
    #[arg(short, long, value_name = "OUT_DIR")]
    pub out_dir: Option<PathBuf>,

    /// Output directory for JSON import file
    #[arg(short, long, value_name = "JSON_DIR", default_value = "import_json")]
    pub json_dir: Option<PathBuf>,

    /// staging or production
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "pr")]
pub struct ProcessTicketArgs {
    /// Ticket ID
    #[arg(
        short,
        long,
        value_name = "ID",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    ticket_id: u64,

    /// Script directory
    #[arg(short('S'), long, value_name = "DIR")]
    pub script_dir: Option<PathBuf>,

    /// Output directory for processed files
    #[arg(short, long, value_name = "OUT_DIR")]
    pub out_dir: Option<PathBuf>,

    /// Output directory for JSON import file
    #[arg(short, long, value_name = "JSON_DIR", default_value = "import_json")]
    pub json_dir: Option<PathBuf>,

    /// staging or production
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "re")]
pub struct ReprocessArgs {
    /// Simulation ID
    #[arg(value_name = "INT")]
    pub simulation_id: u32,

    /// Script directory
    #[arg(short('S'), long, value_name = "SCRIPTS")]
    pub script_dir: Option<PathBuf>,

    /// Output directory for processed files
    #[arg(
        short,
        long,
        value_name = "WORK_DIR",
        default_value = "/opt/mdrepo/reprocess"
    )]
    pub work_dir: PathBuf,

    /// Output directory for JSON import file
    #[arg(short, long, value_name = "JSON_DIR", default_value = "import_json")]
    pub json_dir: Option<PathBuf>,

    /// staging or production
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,
}

// --------------------------------------------------
#[derive(Debug, Clone)]
pub enum Server {
    Production,
    Staging,
}

impl ValueEnum for Server {
    fn value_variants<'a>() -> &'a [Self] {
        &[Server::Production, Server::Staging]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            Server::Production => PossibleValue::new("prod"),
            Server::Staging => PossibleValue::new("staging"),
        })
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Server::Production => "prod",
                Server::Staging => "staging",
            }
        )
    }
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
#[derive(Debug)]
pub struct ProcessedFiles {
    pub full_gro: PathBuf,
    pub full_pdb: PathBuf,
    pub full_xtc: PathBuf,
    pub min_gro: PathBuf,
    pub min_pdb: PathBuf,
    pub min_xtc: PathBuf,
    pub sampled_xtc: PathBuf,
    pub thumbnail_png: PathBuf,
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PdbEntry {
    pub pdb_id: String,
    pub title: String,
    pub classification: String,
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

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbGraphqlResponse {
    pub data: PdbData,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbData {
    pub entry: PdbDataEntry,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbDataEntry {
    pub polymer_entities: Vec<PdbPolymerEntities>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbPolymerEntities {
    pub uniprots: Vec<PdbUniprot>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PdbUniprot {
    pub rcsb_id: String,
    pub rcsb_uniprot_protein: RcsbUniprotProtein,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct RcsbUniprotProtein {
    pub name: RcsbUniprotProteinName,
    pub sequence: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct RcsbUniprotProteinName {
    pub value: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct Export {
    pub simulation: MdSimulation,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct MdSimulation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simulation_id: Option<u32>,
    pub lead_contributor_orcid: String,
    pub unique_file_hash_string: String,
    pub description: String,
    pub short_description: Option<String>,
    pub software: MdSoftware,
    pub run_commands: Option<String>,
    pub pdb: Option<PdbEntry>,
    pub uniprots: Vec<UniprotEntry>,
    pub external_link: Option<String>,
    pub forcefield: Option<String>,
    pub forcefield_comments: Option<String>,
    pub protonation_method: Option<String>,
    pub rmsd_values: Vec<f64>,
    pub rmsf_values: Vec<f64>,
    pub duration: u32,
    pub sampling_frequency: f32,
    pub integration_timestep_fs: f64,
    pub temperature: Option<u32>,
    pub fasta_sequence: String,
    pub replicate: u32,
    pub total_replicates: u32,
    pub includes_water: bool,
    pub water_type: Option<String>,
    pub water_density: Option<f32>,
    pub water_density_units: Option<String>,
    pub topology_hash: String,
    pub contributors: Vec<MdContributor>,
    pub original_files: Vec<MdFile>,
    pub processed_files: Vec<MdFile>,
    pub ligands: Vec<MdLigand>,
    pub solvents: Vec<MdSolvent>,
    pub papers: Vec<MdPaper>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MdPaper {
    pub title: String,

    pub authors: String,

    pub journal: String,

    pub volume: i64,

    pub number: Option<String>,

    pub year: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,

    pub doi: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MdSolvent {
    pub name: String,
    pub concentration: f64,
    pub concentration_units: String,
}

// --------------------------------------------------
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MdLigand {
    pub name: String,
    pub smiles: String,
}

// --------------------------------------------------
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MdFile {
    pub name: String,
    pub file_type: String,
    pub size: u64,
    pub md5_sum: String,
    pub description: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct MdSoftware {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct MdContributor {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,

    pub rank: u32,
}
