use clap::{builder::PossibleValue, Parser, ValueEnum};
use libmdrepo::metadata;
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

// --------------------------------------------------
#[derive(Parser, Debug)]
#[command(arg_required_else_help = true, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

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

    /// Process simulation directory
    Process(ProcessArgs),

    /// Reprocess an existing simulation
    Reprocess(ReprocessArgs),

    /// Use ticket ID to download and process
    Ticket(TicketArgs),
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "pr")]
pub struct ProcessArgs {
    /// Input directory
    #[arg(value_name = "IN_DIR")]
    pub input_dir: PathBuf,

    /// Server
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,

    /// Script directory
    #[arg(short('S'), long, value_name = "SCRIPT_DIR")]
    pub script_dir: Option<PathBuf>,

    /// Working directory
    #[arg(short, long, value_name = "WORK_DIR")]
    pub work_dir: Option<PathBuf>,

    /// Output directory for processed files
    #[arg(short, long, value_name = "OUT_DIR")]
    pub out_dir: Option<PathBuf>,

    /// Simulation ID
    #[arg(long, value_name = "SIMULATION_ID")]
    pub reprocess_simulation_id: Option<u32>,

    /// Allow missing PDB/Uniprot IDs in metadata
    #[arg(short, long)]
    pub no_id: bool,

    /// Force removal of any existing "processed" directory
    #[arg(short, long)]
    pub force: bool,

    /// Process files/create import JSON but do not import/push
    #[arg(short, long)]
    pub dry_run: bool,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "ti")]
pub struct TicketArgs {
    /// Ticket ID
    #[arg(
        short,
        long,
        value_name = "ID",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub ticket_id: u64,

    /// Server
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,

    /// Script directory
    #[arg(short('S'), long, value_name = "SCRIPT_DIR")]
    pub script_dir: Option<PathBuf>,

    /// Root working directory for MDRepo
    #[arg(short, long, value_name = "WORK_DIR")]
    pub work_dir: Option<PathBuf>,

    /// Force removal of any existing "processed" directory
    #[arg(short, long)]
    pub force: bool,

    /// Process files/create import JSON but do not import/push
    #[arg(short, long)]
    pub dry_run: bool,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "re")]
pub struct ReprocessArgs {
    /// Simulation ID
    #[arg(value_name = "SIM_ID")]
    pub simulation_id: u32,

    /// Server
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,

    /// Script directory
    #[arg(short('S'), long, value_name = "SCRIPT_DIR")]
    pub script_dir: Option<PathBuf>,

    /// Output directory for processed files
    #[arg(short, long, value_name = "WORK_DIR")]
    pub work_dir: Option<PathBuf>,

    /// Preserve working directory
    #[arg(long)]
    pub preserve: bool,

    /// Force removal of any existing "processed" directory
    #[arg(short, long)]
    pub force: bool,

    /// Process files/create import JSON but do not import/push
    #[arg(short, long)]
    pub dry_run: bool,
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

    /// Allow missing PDB/Uniprot IDs in metadata
    #[arg(short, long)]
    pub no_id: bool,
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
    pub simulation: ExportSimulation,
    pub warnings: Vec<String>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct ExportSimulation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simulation_id: Option<u32>,
    pub lead_contributor_orcid: String,
    pub unique_file_hash_string: String,
    pub user_accession: Option<String>,
    pub short_description: String,
    pub description: Option<String>,
    pub software_name: String,
    pub software_version: String,
    pub run_commands: Option<String>,
    pub pdb: Option<PdbEntry>,
    pub uniprots: Vec<UniprotEntry>,
    pub external_links: Vec<metadata::ExternalLink>,
    pub forcefield: Option<String>,
    pub forcefield_comments: Option<String>,
    pub protonation_method: Option<String>,
    pub rmsd_values: Vec<f64>,
    pub rmsf_values: Vec<f64>,
    pub duration: u32,
    pub sampling_frequency: f32,
    pub integration_timestep_fs: u32,
    pub temperature_kelvin: u32,
    pub fasta_sequence: String,
    pub water: Option<metadata::Water>,
    pub structure_hash: String,
    pub contributors: Vec<metadata::Contributor>,
    pub original_files: Vec<MdFile>,
    pub processed_files: Vec<MdFile>,
    pub ligands: Vec<metadata::Ligand>,
    pub solutes: Vec<metadata::Solute>,
    pub papers: Vec<metadata::Paper>,
}

// --------------------------------------------------
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MdFile {
    pub name: String,

    pub file_type: String,

    pub size: u64,

    pub md5_sum: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_primary: Option<bool>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct ImportResult {
    pub server: String,
    pub filename: String,
    pub simulation_id: u32,
    pub data_dir: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct PushResult {
    pub src: String,
    pub dest: String,
    pub size: u64,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct DoiPaper {
    pub title: String,
    pub author: Vec<DoiAuthor>,
    pub journal: String,
    pub volume: u32,
    pub page: String,
    pub published: DoiPublishedDateParts,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct DoiAuthor {
    pub family: String,
    pub given: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct DoiPublishedDateParts {
    #[serde(alias = "date-parts")]
    pub date_parts: Vec<u32>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct TicketInfo {
    pub first_name: String,
    pub last_name: Option<String>,
    pub email: String,
    pub institution: Option<String>,
    pub orcid: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct InferredLigand {
    pub structure: InferredLigandStructure,
    pub name: InferredLigandName,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct InferredLigandName {
    pub smiles_input: String,
    pub cid: Option<u32>,
    pub iupac_name: Option<String>,
    pub common_name: Option<String>,
    pub formula: Option<String>,
    pub charge: Option<u32>,
    pub synonyms: Option<Vec<String>>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct InferredLigandStructure {
    pub smiles: String,
    pub formula: String,
    pub num_atoms: u32,
    pub num_heavy_atoms: u32,
    pub charge: u32,
    pub inchikey: String,
    pub resname: String,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct CheckedLigand {
    pub smi1_canonical: String,
    pub smi2_canonical: String,
    pub formula1: String,
    pub formula2: String,
    pub charge1: u32,
    pub charge2: u32,
    pub exact_match: bool,
    pub same_connectivity: bool,
    pub same_connectivity_and_stereo: bool,
    pub same_inchi: bool,
    pub differences: Vec<String>,
    pub inchi1: String,
    pub inchi2: String,
    pub connectivity_layer: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct BlastResult {
    pub qaccver: u32,
    pub saccver: String,
    pub pident: f64,
    pub length: u32,
    pub mismatch: u32,
    pub gapopen: u32,
    pub qstart: u32,
    pub qend: u32,
    pub sstart: u32,
    pub send: u32,
    pub evalue: f64,
    pub bitscore: f64,
}

// --------------------------------------------------
#[derive(Debug, strum_macros::Display)]
pub enum UniprotDb {
    Swissprot,
    Trembl,
}
