use clap::Parser;
use libmdrepo::metadata::{self, Meta};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};

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

    /// Number of threads
    #[arg(short('t'), long)]
    pub num_threads: Option<usize>,
}

// --------------------------------------------------
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum LogLevel {
    Info,
    Debug,
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
    pub reprocess_simulation_id: Option<u64>,

    /// Allow missing PDB/Uniprot IDs in metadata
    #[arg(short, long)]
    pub no_id: bool,

    /// Force removal of any existing "processed" directory
    #[arg(short, long)]
    pub force: bool,

    /// Process files/create import JSON but do not import/push
    #[arg(short, long)]
    pub dry_run: bool,

    /// Replace original files
    #[arg(long)]
    pub replace_original_files: bool,
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

    /// Skip file download
    #[arg(long)]
    pub skip_download: bool,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "re")]
pub struct ReprocessArgs {
    /// Simulation ID
    #[arg(value_name = "SIM_ID")]
    pub simulation_id: String,

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
#[derive(Debug, Clone, clap::ValueEnum, strum_macros::Display)]
pub enum Server {
    #[value(name = "prod")]
    #[strum(serialize = "prod")]
    Production,
    #[strum(serialize = "staging")]
    Staging,
}

// --------------------------------------------------
#[derive(Debug, Parser)]
#[command(alias = "va")]
pub struct ValidateArgs {
    /// Input directory
    #[arg(value_name = "DIR", num_args = 1..)]
    pub dirnames: Vec<PathBuf>,

    /// Allow missing PDB/Uniprot IDs in metadata
    #[arg(short, long)]
    pub no_id: bool,
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
pub struct ProcessTrajectoryArgs<'a> {
    pub trajectory_num: usize,
    pub trajectory_file_name: &'a str,
    pub structure_file_name: &'a str,
    pub topology_file_name: &'a str,
    pub input_dir: &'a Path,
    pub processed_dir: &'a Path,
    pub script_dir: &'a Path,
    pub uv: &'a Path,
    /// Prefix of the `simproc` conda env, discovered once at startup.
    /// We invoke its `bin/python` directly (and set PATH/AMBERHOME) instead of
    /// going through `micromamba run`, which serializes on a global lock.
    pub simproc_prefix: &'a Path,
}

// --------------------------------------------------
#[derive(Debug)]
pub struct ProcessedTrajectory {
    pub full_gro: PathBuf,
    pub full_pdb: PathBuf,
    pub full_xtc: PathBuf,
    pub min_gro: PathBuf,
    pub min_pdb: PathBuf,
    pub min_xtc: PathBuf,
    pub sampled_xtc: PathBuf,
    pub thumbnail_png: PathBuf,
    pub full_xtc_size: u64,
    pub trajectory_file_name: String,
    pub trajectory_file_stem: String,
    pub directory_name: String,
    pub is_coarse_grained: bool,
}

// --------------------------------------------------
#[derive(Debug)]
pub struct ProcessedTarball {
    pub path: PathBuf,
    pub file_type: String,
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
    pub totaltime_ns: f64,
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
/// The `import.json` payload. Kept on disk as the audit artifact of a run and
/// read by `push_sim_files.py`, which wants the `simulation` key.
#[derive(Debug, Deserialize, Serialize)]
pub struct Export {
    pub simulation: ExportSimulation,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct ExportSimulation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simulation_id: Option<u64>,

    pub lead_contributor_orcid: String,

    pub unique_file_hash_string: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    pub short_description: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    pub software_name: String,

    pub software_version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_commands: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdb: Option<PdbEntry>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uniprots: Vec<UniprotEntry>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<metadata::ExternalLink>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield_comments: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub protonation_method: Option<String>,

    pub rmsd_values: Vec<f64>,

    pub rmsf_values: Vec<f64>,

    pub duration: f64,

    pub sampling_frequency: f32,

    pub integration_timestep_fs: u32,

    pub temperature_kelvin: u32,

    pub fasta_sequence: String,

    pub num_replicates: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_density: Option<f64>,

    pub structure_hash: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contributors: Vec<metadata::Contributor>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub original_files: Vec<MdFile>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub processed_files: Vec<MdFile>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replicates: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ligands: Vec<metadata::Ligand>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub solutes: Vec<metadata::Solute>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub papers: Vec<metadata::Paper>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_embargoed: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_coarse_grained: Option<bool>,
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
#[derive(Debug)]
pub struct RunImportArgs<'a> {
    pub simulation: &'a ExportSimulation,
    pub server: &'a Server,
    pub reprocess_simulation_id: Option<u64>,
    pub replace_original_files: bool,
}

// --------------------------------------------------
/// What the importer hands to `push_sim_files.py`.
#[derive(Debug)]
pub struct ImportResult {
    /// The `import.json` the push script re-reads the simulation from.
    pub filename: String,
    pub simulation_id: u32,
}

// --------------------------------------------------
#[derive(Debug)]
pub struct ProcessResult {
    /// The imported simulation's ID. `None` on a dry run (nothing imported).
    pub simulation_id: Option<u32>,

    /// Non-fatal errors/warnings collected during processing.
    pub warnings: Vec<String>,
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
    #[serde(deserialize_with = "de_text")]
    pub title: String,

    #[serde(default)]
    pub author: Vec<DoiAuthor>,

    #[serde(default, deserialize_with = "de_opt_text")]
    pub publisher: Option<String>,

    #[serde(rename = "URL")]
    pub url: Option<String>,

    #[serde(default, deserialize_with = "de_opt_text")]
    pub journal: Option<String>,

    #[serde(rename = "container-title", default, deserialize_with = "de_opt_text")]
    pub container_title: Option<String>,

    pub volume: Option<String>,

    #[serde(default, deserialize_with = "de_opt_text")]
    pub page: Option<String>,

    /// Journals that number articles rather than paginating them (Scientific
    /// Data, Nature Communications, Scientific Reports, ...) report an
    /// `article-number` and carry no `page` at all.
    #[serde(rename = "article-number", default, deserialize_with = "de_opt_text")]
    pub article_number: Option<String>,

    #[serde(default, deserialize_with = "de_opt_text")]
    pub issue: Option<String>,

    /// Some records report the issue only here, not at the top level.
    #[serde(rename = "journal-issue")]
    pub journal_issue: Option<DoiJournalIssue>,

    pub published: Option<DoiDateParts>,

    pub issued: Option<DoiDateParts>,

    #[serde(rename = "published-print")]
    pub published_print: Option<DoiDateParts>,

    #[serde(rename = "published-online")]
    pub published_online: Option<DoiDateParts>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct DoiJournalIssue {
    #[serde(default, deserialize_with = "de_opt_text")]
    pub issue: Option<String>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct DoiAuthor {
    #[serde(default, deserialize_with = "de_opt_text")]
    pub family: Option<String>,

    #[serde(default, deserialize_with = "de_opt_text")]
    pub given: Option<String>,

    /// Consortium and organization authors carry a single `name` and neither a
    /// `given` nor a `family`.
    #[serde(default, deserialize_with = "de_opt_text")]
    pub name: Option<String>,
}

impl DoiAuthor {
    /// "Given Family", or whichever half the record has; a consortium author
    /// falls back to its `name`. `None` when the entry names nobody, so that
    /// such an entry drops out of the author list rather than joining it as an
    /// empty string.
    pub fn display_name(&self) -> Option<String> {
        let parts: Vec<&str> = [self.given.as_deref(), self.family.as_deref()]
            .into_iter()
            .flatten()
            .collect();

        if parts.is_empty() {
            self.name.clone()
        } else {
            Some(parts.join(" "))
        }
    }
}

// --------------------------------------------------
/// A DOI text field arrives as either a string or a list of strings; Crossref
/// sends `title` and `container-title` both ways.
#[derive(Deserialize)]
#[serde(untagged)]
enum DoiText {
    One(String),
    Many(Vec<String>),
}

// --------------------------------------------------
/// Normalize a DOI text field: take the first value of a list, undo Crossref's
/// XML escaping ("Nature Structural &amp; Molecular Biology") and collapse the
/// whitespace it uses to pretty-print titles that contain markup across several
/// indented lines. Both have to be undone here or the escaped, ragged form is
/// what lands in `md_pub`.
fn clean_text(text: DoiText) -> Option<String> {
    let value = match text {
        DoiText::One(val) => Some(val),
        DoiText::Many(vals) => vals.into_iter().next(),
    }?;

    let collapsed = unescape_entities(&value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    (!collapsed.is_empty()).then_some(collapsed)
}

// --------------------------------------------------
/// Replace the XML entities Crossref escapes with the characters they stand
/// for. Anything that is not a known entity is left as written, so a bare `&`
/// survives intact.
fn unescape_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];

        let decoded = rest
            .find(';')
            .and_then(|semi| Some((decode_entity(&rest[1..semi])?, semi)));

        match decoded {
            Some((ch, semi)) => {
                out.push(ch);
                rest = &rest[semi + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }

    out.push_str(rest);

    out
}

// --------------------------------------------------
/// Decode the body of an entity -- the part between `&` and `;` -- whether it
/// is named (`amp`) or numeric (`#39`, `#x27`).
fn decode_entity(body: &str) -> Option<char> {
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let digits = body.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                _ => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

// --------------------------------------------------
fn de_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(clean_text(DoiText::deserialize(deserializer)?).unwrap_or_default())
}

// --------------------------------------------------
fn de_opt_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<DoiText>::deserialize(deserializer)?.and_then(clean_text))
}

// --------------------------------------------------
/// Crossref/DOI dates arrive as `{"date-parts": [[year, month, day]]}`: a list
/// of date tuples (usually one), each `[year]`, `[year, month]`, or a full date.
/// Used for both the `published` and `issued` fields, which share this shape.
#[derive(Debug, Deserialize, Serialize)]
pub struct DoiDateParts {
    #[serde(alias = "date-parts")]
    pub date_parts: Vec<Vec<u32>>,
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
    pub inchikey: Option<String>,
    pub wikidata_label: Option<String>,
    pub pubchem_title: Option<String>,
    pub best_name: Option<String>,
    pub name_source: Option<String>,
    pub common_name: Option<String>,
    pub formula: Option<String>,
    pub charge: Option<i32>,
    pub synonyms: Option<Vec<String>>,
}

// --------------------------------------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct InferredLigandStructure {
    pub smiles: String,
    pub formula: String,
    pub num_atoms: u32,
    pub num_heavy_atoms: u32,
    pub charge: i32,
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
    pub charge1: i32,
    pub charge2: i32,
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
    Isoform,
    Trembl,
}

// --------------------------------------------------
#[derive(Debug, strum_macros::EnumIter, strum_macros::Display)]
pub enum ProcessedTrajectoryType {
    Full,
    Minimal,
    Sampled,
}

// --------------------------------------------------
#[derive(Debug)]
pub struct ImportJsonArgs<'a> {
    pub meta: Meta,
    pub import_json: &'a Path,
    pub processed_dir: &'a Path,
    pub meta_path: &'a Path,
    pub input_dir: &'a Path,
    pub script_dir: &'a Path,
    pub blast_dir: &'a Path,
    pub uv: &'a Path,
    pub example_trajectory: &'a ProcessedTrajectory,
    pub all_trajectories: &'a [ProcessedTrajectory],
    pub trajectory_tarballs: &'a [ProcessedTarball],
    pub reprocess_simulation_id: Option<u64>,
    pub replicates: &'a [String],
    pub replace_original_files: bool,
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::{DoiAuthor, DoiPaper};
    use anyhow::Result;
    use std::fs;

    #[test]
    fn test_doi_paper() -> Result<()> {
        let text = fs::read_to_string("tests/inputs/doi.json")?;
        let paper: DoiPaper = serde_json::from_str(&text)?;
        assert_eq!(paper.publisher, Some("arXiv".to_string()));
        Ok(())
    }

    // Regression: a Crossref/DOI response (10.1038/s41597-024-04140-z) where
    // `volume` is a string ("11") and `published.date-parts` is nested
    // ([[2024,11,28]]). Both used to fail to deserialize into DoiPaper.
    #[test]
    fn test_doi_paper_crossref_string_volume_and_nested_dates() -> Result<()> {
        let text = fs::read_to_string("tests/inputs/doi-crossref.json")?;
        let paper: DoiPaper = serde_json::from_str(&text)?;
        assert_eq!(paper.volume, Some("11".to_string()));
        assert_eq!(
            paper.published.unwrap().date_parts,
            vec![vec![2024, 11, 28]]
        );
        assert_eq!(paper.issued.unwrap().date_parts, vec![vec![2024, 11, 28]]);
        // The journal is `container-title`, not the publisher.
        assert_eq!(paper.container_title, Some("Scientific Data".to_string()));
        assert_eq!(
            paper.publisher,
            Some("Springer Science and Business Media LLC".to_string())
        );
        Ok(())
    }

    // The same record is also the article-number case: Scientific Data numbers
    // articles instead of paginating them, so it has no `page` at all.
    #[test]
    fn test_doi_paper_issue_and_article_number() -> Result<()> {
        let text = fs::read_to_string("tests/inputs/doi-crossref.json")?;
        let paper: DoiPaper = serde_json::from_str(&text)?;
        assert_eq!(paper.issue, Some("1".to_string()));
        assert_eq!(paper.journal_issue.unwrap().issue, Some("1".to_string()));
        assert_eq!(paper.page, None);
        assert_eq!(paper.article_number, Some("1299".to_string()));
        Ok(())
    }

    // Crossref sends `title` and `container-title` as either a string or a
    // list, escapes markup, and pretty-prints titles that contain markup across
    // several indented lines. All three used to reach `md_pub` as written -- or,
    // for the list, to fail to deserialize and drop the paper entirely.
    #[test]
    fn test_doi_paper_list_fields_and_escapes() -> Result<()> {
        let text = r#"{
            "title": [
                "Structure of the\n                 protein &amp; its ligand"
            ],
            "container-title": ["Nature Structural &amp; Molecular Biology"],
            "author": []
        }"#;
        let paper: DoiPaper = serde_json::from_str(text)?;
        assert_eq!(paper.title, "Structure of the protein & its ligand");
        assert_eq!(
            paper.container_title,
            Some("Nature Structural & Molecular Biology".to_string())
        );
        Ok(())
    }

    // An empty list is as good as an absent field, and a bare `&` is not an
    // entity: it has to survive unescaping untouched.
    #[test]
    fn test_doi_paper_empty_list_and_bare_ampersand() -> Result<()> {
        let text = r#"{
            "title": "Ligand binding in AT&T's assay (&#37; bound)",
            "container-title": [],
            "author": []
        }"#;
        let paper: DoiPaper = serde_json::from_str(text)?;
        assert_eq!(paper.title, "Ligand binding in AT&T's assay (% bound)");
        assert_eq!(paper.container_title, None);
        Ok(())
    }

    // A consortium author carries a `name` and neither `given` nor `family`,
    // and an author may carry only one of the two halves. Both used to fail to
    // deserialize, taking the whole record with them.
    #[test]
    fn test_doi_author_partial_names() -> Result<()> {
        let text = r#"{
            "title": "A collaborative study",
            "author": [
                {"given": "Ada", "family": "Lovelace"},
                {"family": "Hopper"},
                {"given": "Grace"},
                {"name": "The MDRepo Consortium"},
                {"ORCID": "https://orcid.org/0000-0000-0000-0000"}
            ]
        }"#;
        let paper: DoiPaper = serde_json::from_str(text)?;
        let names: Vec<Option<String>> =
            paper.author.iter().map(DoiAuthor::display_name).collect();
        assert_eq!(
            names,
            vec![
                Some("Ada Lovelace".to_string()),
                Some("Hopper".to_string()),
                Some("Grace".to_string()),
                Some("The MDRepo Consortium".to_string()),
                None,
            ]
        );
        Ok(())
    }
}
