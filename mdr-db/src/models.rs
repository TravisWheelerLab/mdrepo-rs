#![allow(dead_code)]

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::schema::*;

// ── md_contribution ───────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_contribution)]
#[diesel(belongs_to(Simulation))]
pub struct Contribution {
    pub id: i64,
    pub email: Option<String>,
    pub institution: Option<String>,
    pub name: Option<String>,
    pub orcid: Option<String>,
    pub simulation_id: Option<i64>,
    pub rank: i32,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_contribution)]
pub struct NewContribution {
    pub email: Option<String>,
    pub institution: Option<String>,
    pub name: Option<String>,
    pub orcid: Option<String>,
    pub simulation_id: Option<i64>,
    pub rank: i32,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_contribution)]
pub struct ContributionUpdate {
    pub email: Option<Option<String>>,
    pub institution: Option<Option<String>>,
    pub name: Option<Option<String>>,
    pub orcid: Option<Option<String>>,
    pub simulation_id: Option<Option<i64>>,
    pub rank: Option<i32>,
}

// ── md_external_link ──────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_external_link)]
#[diesel(belongs_to(Simulation))]
pub struct ExternalLink {
    pub id: i64,
    pub url: String,
    pub label: Option<String>,
    pub simulation_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_external_link)]
pub struct NewExternalLink {
    pub url: String,
    pub label: Option<String>,
    pub simulation_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_external_link)]
pub struct ExternalLinkUpdate {
    pub url: Option<String>,
    pub label: Option<Option<String>>,
    pub simulation_id: Option<i64>,
}

// ── md_feature_switch ─────────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_feature_switch)]
pub struct FeatureSwitch {
    pub id: i64,
    pub irods_service_available: bool,
    pub simulation_animation_available: bool,
    pub media_service: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_feature_switch)]
pub struct NewFeatureSwitch {
    pub irods_service_available: bool,
    pub simulation_animation_available: bool,
    pub media_service: String,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_feature_switch)]
pub struct FeatureSwitchUpdate {
    pub irods_service_available: Option<bool>,
    pub simulation_animation_available: Option<bool>,
    pub media_service: Option<String>,
}

// ── md_frontend_download_instance ────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_frontend_download_instance)]
#[diesel(belongs_to(Simulation))]
#[diesel(belongs_to(User))]
pub struct DownloadInstance {
    pub id: i64,
    pub created_on: DateTime<Utc>,
    pub used: bool,
    pub simulation_id: i64,
    pub user_id: Option<i64>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_frontend_download_instance)]
pub struct NewDownloadInstance {
    pub created_on: DateTime<Utc>,
    pub used: bool,
    pub simulation_id: i64,
    pub user_id: Option<i64>,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_frontend_download_instance)]
pub struct DownloadInstanceUpdate {
    pub used: Option<bool>,
    pub simulation_id: Option<i64>,
    pub user_id: Option<Option<i64>>,
}

// ── md_frontend_download_instance_processed_files ────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_frontend_download_instance_processed_files)]
#[diesel(belongs_to(DownloadInstance, foreign_key = frontenddownloadinstance_id))]
#[diesel(belongs_to(ProcessedFile, foreign_key = simulationprocessedfile_id))]
pub struct DownloadProcessedFile {
    pub id: i64,
    pub frontenddownloadinstance_id: i64,
    pub simulationprocessedfile_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_frontend_download_instance_processed_files)]
pub struct NewDownloadProcessedFile {
    pub frontenddownloadinstance_id: i64,
    pub simulationprocessedfile_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_frontend_download_instance_processed_files)]
pub struct DownloadProcessedFileUpdate {
    pub frontenddownloadinstance_id: Option<i64>,
    pub simulationprocessedfile_id: Option<i64>,
}

// ── md_frontend_download_instance_uploaded_files ─────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_frontend_download_instance_uploaded_files)]
#[diesel(belongs_to(DownloadInstance, foreign_key = frontenddownloadinstance_id))]
#[diesel(belongs_to(UploadedFile, foreign_key = simulationuploadedfile_id))]
pub struct DownloadUploadedFile {
    pub id: i64,
    pub frontenddownloadinstance_id: i64,
    pub simulationuploadedfile_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_frontend_download_instance_uploaded_files)]
pub struct NewDownloadUploadedFile {
    pub frontenddownloadinstance_id: i64,
    pub simulationuploadedfile_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_frontend_download_instance_uploaded_files)]
pub struct DownloadUploadedFileUpdate {
    pub frontenddownloadinstance_id: Option<i64>,
    pub simulationuploadedfile_id: Option<i64>,
}

// ── md_ligand ─────────────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_ligand)]
#[diesel(belongs_to(Simulation))]
pub struct Ligand {
    pub id: i64,
    pub name: String,
    pub smiles_string: String,
    pub simulation_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_ligand)]
pub struct NewLigand {
    pub name: String,
    pub smiles_string: String,
    pub simulation_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_ligand)]
pub struct LigandUpdate {
    pub name: Option<String>,
    pub smiles_string: Option<String>,
    pub simulation_id: Option<i64>,
}

// ── md_pdb ────────────────────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_pdb)]
pub struct Pdb {
    pub id: i64,
    pub pdb_id: String,
    pub classification: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_pdb)]
pub struct NewPdb {
    pub pdb_id: String,
    pub classification: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_pdb)]
pub struct PdbUpdate {
    pub pdb_id: Option<String>,
    pub classification: Option<Option<String>>,
    pub title: Option<Option<String>>,
}

// ── md_processed_file ─────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_processed_file)]
#[diesel(belongs_to(Simulation))]
pub struct ProcessedFile {
    pub id: i64,
    pub file_type: String,
    pub local_file_path: String,
    pub filename: String,
    pub simulation_id: i64,
    pub file_size_bytes: Option<i64>,
    pub description: Option<String>,
    pub md5_hash: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_processed_file)]
pub struct NewProcessedFile {
    pub file_type: String,
    pub local_file_path: String,
    pub filename: String,
    pub simulation_id: i64,
    pub file_size_bytes: Option<i64>,
    pub description: Option<String>,
    pub md5_hash: Option<String>,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_processed_file)]
pub struct ProcessedFileUpdate {
    pub file_type: Option<String>,
    pub local_file_path: Option<String>,
    pub filename: Option<String>,
    pub simulation_id: Option<i64>,
    pub file_size_bytes: Option<Option<i64>>,
    pub description: Option<Option<String>>,
    pub md5_hash: Option<Option<String>>,
}

// ── md_pub ────────────────────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_pub)]
pub struct Pub {
    pub id: i64,
    pub title: String,
    pub authors: String,
    pub journal: String,
    pub volume: i32,
    pub number: Option<String>,
    pub year: i32,
    pub pages: Option<String>,
    pub doi: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_pub)]
pub struct NewPub {
    pub title: String,
    pub authors: String,
    pub journal: String,
    pub volume: i32,
    pub number: Option<String>,
    pub year: i32,
    pub pages: Option<String>,
    pub doi: String,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_pub)]
pub struct PubUpdate {
    pub title: Option<String>,
    pub authors: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<i32>,
    pub number: Option<Option<String>>,
    pub year: Option<i32>,
    pub pages: Option<Option<String>>,
    pub doi: Option<String>,
}

// ── md_simulation ─────────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_simulation)]
#[diesel(belongs_to(Software))]
#[diesel(belongs_to(Ticket, foreign_key = md_repo_ticket_id))]
#[diesel(belongs_to(User, foreign_key = created_by_id))]
#[diesel(belongs_to(ReplicateGroup))]
#[diesel(belongs_to(Pdb))]
pub struct Simulation {
    pub id: i64,
    pub description: Option<String>,
    pub run_commands: Option<String>,
    pub water_type: Option<String>,
    pub water_density: Option<f64>,
    pub duration: Option<f64>,
    pub sampling_frequency: Option<f64>,
    pub creation_date: DateTime<Utc>,
    pub software_id: Option<i64>,
    pub md_repo_ticket_id: Option<i64>,
    pub rmsd_values: Option<Vec<f64>>,
    pub rmsf_values: Option<Vec<f64>>,
    pub is_placeholder: bool,
    pub created_by_id: Option<i64>,
    pub replicate_group_id: Option<i64>,
    pub unique_file_hash_string: Option<String>,
    pub forcefield: Option<String>,
    pub forcefield_comments: Option<String>,
    pub temperature: Option<i32>,
    pub is_deprecated: bool,
    pub protonation_method: Option<String>,
    pub integration_timestep_fs: Option<i32>,
    pub short_description: Option<String>,
    pub pdb_id: Option<i64>,
    pub is_public: bool,
    pub fasta_sequence: Option<String>,
    pub user_accession: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_simulation)]
pub struct NewSimulation {
    pub description: Option<String>,
    pub short_description: Option<String>,
    pub run_commands: Option<String>,
    pub water_type: Option<String>,
    pub water_density: Option<f64>,
    pub duration: Option<f64>,
    pub sampling_frequency: Option<f64>,
    pub creation_date: DateTime<Utc>,
    pub software_id: Option<i64>,
    pub forcefield: Option<String>,
    pub forcefield_comments: Option<String>,
    pub temperature: Option<i32>,
    pub is_placeholder: bool,
    pub is_deprecated: bool,
    pub is_public: bool,
    pub protonation_method: Option<String>,
    pub fasta_sequence: Option<String>,
    pub user_accession: Option<String>,
    pub pdb_id: Option<i64>,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_simulation)]
pub struct SimulationUpdate {
    pub description: Option<Option<String>>,
    pub short_description: Option<Option<String>>,
    pub run_commands: Option<Option<String>>,
    pub water_type: Option<Option<String>>,
    pub water_density: Option<Option<f64>>,
    pub duration: Option<Option<f64>>,
    pub sampling_frequency: Option<Option<f64>>,
    pub forcefield: Option<Option<String>>,
    pub forcefield_comments: Option<Option<String>>,
    pub temperature: Option<Option<i32>>,
    pub is_placeholder: Option<bool>,
    pub is_deprecated: Option<bool>,
    pub is_public: Option<bool>,
    pub protonation_method: Option<Option<String>>,
    pub fasta_sequence: Option<Option<String>>,
    pub user_accession: Option<Option<String>>,
    pub pdb_id: Option<Option<i64>>,
}

// ── md_simulation_pub ─────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_simulation_pub)]
#[diesel(belongs_to(Simulation))]
#[diesel(belongs_to(Pub))]
pub struct SimulationPub {
    pub id: i64,
    pub simulation_id: i64,
    pub pub_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_simulation_pub)]
pub struct NewSimulationPub {
    pub simulation_id: i64,
    pub pub_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_simulation_pub)]
pub struct SimulationPubUpdate {
    pub simulation_id: Option<i64>,
    pub pub_id: Option<i64>,
}

// ── md_simulation_replicate_group ─────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_simulation_replicate_group)]
pub struct ReplicateGroup {
    pub id: i64,
    pub psf_hash: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_simulation_replicate_group)]
pub struct NewReplicateGroup {
    pub psf_hash: String,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_simulation_replicate_group)]
pub struct ReplicateGroupUpdate {
    pub psf_hash: Option<String>,
}

// ── md_simulation_uniprot ─────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_simulation_uniprot)]
#[diesel(belongs_to(Simulation))]
#[diesel(belongs_to(Uniprot))]
pub struct SimulationUniprot {
    pub id: i64,
    pub simulation_id: i64,
    pub uniprot_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_simulation_uniprot)]
pub struct NewSimulationUniprot {
    pub simulation_id: i64,
    pub uniprot_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_simulation_uniprot)]
pub struct SimulationUniprotUpdate {
    pub simulation_id: Option<i64>,
    pub uniprot_id: Option<i64>,
}

// ── md_software ───────────────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_software)]
pub struct Software {
    pub id: i64,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_software)]
pub struct NewSoftware {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_software)]
pub struct SoftwareUpdate {
    pub name: Option<String>,
    pub version: Option<Option<String>>,
}

// ── md_solvent ────────────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_solvent)]
#[diesel(belongs_to(Simulation))]
pub struct Solvent {
    pub id: i64,
    pub name: String,
    pub concentration: f64,
    pub simulation_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_solvent)]
pub struct NewSolvent {
    pub name: String,
    pub concentration: f64,
    pub simulation_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_solvent)]
pub struct SolventUpdate {
    pub name: Option<String>,
    pub concentration: Option<f64>,
    pub simulation_id: Option<i64>,
}

// ── md_submission_completed_event ─────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_submission_completed_event)]
pub struct SubmissionEvent {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_submission_completed_event)]
pub struct NewSubmissionEvent {
    pub created_at: DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_submission_completed_event)]
pub struct SubmissionEventUpdate {
    pub path: Option<String>,
}

// ── md_ticket ─────────────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_ticket)]
#[diesel(belongs_to(User, foreign_key = created_by_id))]
pub struct Ticket {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub token: String,
    pub full_token: String,
    pub irods_tickets: Option<String>,
    pub guid: Uuid,
    pub n_submissions: i32,
    pub created_by_id: i64,
    pub used_for_upload: bool,
    pub irods_creation_error: bool,
    pub ticket_type: String,
    pub no_files_found: bool,
    pub finished_generating: bool,
    pub orcid: Option<String>,
    pub upload_notification_sent: bool,
    pub processing_complete: bool,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_ticket)]
pub struct NewTicket {
    pub created_at: DateTime<Utc>,
    pub token: String,
    pub full_token: String,
    pub irods_tickets: Option<String>,
    pub guid: Uuid,
    pub n_submissions: i32,
    pub created_by_id: i64,
    pub used_for_upload: bool,
    pub irods_creation_error: bool,
    pub ticket_type: String,
    pub no_files_found: bool,
    pub finished_generating: bool,
    pub orcid: Option<String>,
    pub upload_notification_sent: bool,
    pub processing_complete: bool,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_ticket)]
pub struct TicketUpdate {
    pub token: Option<String>,
    pub full_token: Option<String>,
    pub irods_tickets: Option<Option<String>>,
    pub n_submissions: Option<i32>,
    pub used_for_upload: Option<bool>,
    pub irods_creation_error: Option<bool>,
    pub ticket_type: Option<String>,
    pub no_files_found: Option<bool>,
    pub finished_generating: Option<bool>,
    pub orcid: Option<Option<String>>,
    pub upload_notification_sent: Option<bool>,
    pub processing_complete: Option<bool>,
}

// ── md_uniprot ────────────────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_uniprot)]
pub struct Uniprot {
    pub id: i64,
    pub uniprot_id: String,
    pub name: String,
    pub amino_length: i32,
    pub sequence: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_uniprot)]
pub struct NewUniprot {
    pub uniprot_id: String,
    pub name: String,
    pub amino_length: i32,
    pub sequence: String,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_uniprot)]
pub struct UniprotUpdate {
    pub uniprot_id: Option<String>,
    pub name: Option<String>,
    pub amino_length: Option<i32>,
    pub sequence: Option<String>,
}

// ── md_upload_instance ────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_upload_instance)]
#[diesel(belongs_to(Simulation))]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Ticket))]
pub struct UploadInstance {
    pub id: i64,
    pub created_on: DateTime<Utc>,
    pub simulation_id: Option<i64>,
    pub user_id: Option<i64>,
    pub successful: Option<bool>,
    pub lead_contributor_orcid: String,
    pub filenames: Option<String>,
    pub ticket_id: Option<i64>,
    pub landing_id: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_upload_instance)]
pub struct NewUploadInstance {
    pub created_on: DateTime<Utc>,
    pub simulation_id: Option<i64>,
    pub user_id: Option<i64>,
    pub successful: Option<bool>,
    pub lead_contributor_orcid: String,
    pub filenames: Option<String>,
    pub ticket_id: Option<i64>,
    pub landing_id: Option<String>,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_upload_instance)]
pub struct UploadInstanceUpdate {
    pub simulation_id: Option<Option<i64>>,
    pub user_id: Option<Option<i64>>,
    pub successful: Option<Option<bool>>,
    pub lead_contributor_orcid: Option<String>,
    pub filenames: Option<Option<String>>,
    pub ticket_id: Option<Option<i64>>,
    pub landing_id: Option<Option<String>>,
}

// ── md_upload_instance_message ────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_upload_instance_message)]
#[diesel(belongs_to(UploadInstance, foreign_key = simulation_upload_id))]
pub struct UploadMessage {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub simulation_upload_id: i64,
    pub is_error: bool,
    pub is_warning: bool,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_upload_instance_message)]
pub struct NewUploadMessage {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub simulation_upload_id: i64,
    pub is_error: bool,
    pub is_warning: bool,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_upload_instance_message)]
pub struct UploadMessageUpdate {
    pub message: Option<String>,
    pub is_error: Option<bool>,
    pub is_warning: Option<bool>,
}

// ── md_uploaded_file ──────────────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = md_uploaded_file)]
#[diesel(belongs_to(Simulation))]
pub struct UploadedFile {
    pub id: i64,
    pub filename: String,
    pub file_type: String,
    pub simulation_id: i64,
    pub description: Option<String>,
    pub local_file_path: String,
    pub file_size_bytes: Option<i64>,
    pub md5_hash: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_uploaded_file)]
pub struct NewUploadedFile {
    pub filename: String,
    pub file_type: String,
    pub simulation_id: i64,
    pub description: Option<String>,
    pub local_file_path: String,
    pub file_size_bytes: Option<i64>,
    pub md5_hash: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_uploaded_file)]
pub struct UploadedFileUpdate {
    pub filename: Option<String>,
    pub file_type: Option<String>,
    pub simulation_id: Option<i64>,
    pub description: Option<Option<String>>,
    pub local_file_path: Option<String>,
    pub file_size_bytes: Option<Option<i64>>,
    pub md5_hash: Option<Option<String>>,
    pub is_primary: Option<bool>,
}

// ── md_user ───────────────────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = md_user)]
pub struct User {
    pub id: i64,
    pub password: String,
    pub last_login: Option<DateTime<Utc>>,
    pub is_superuser: bool,
    pub username: String,
    pub is_staff: bool,
    pub date_joined: DateTime<Utc>,
    pub first_name: String,
    pub last_name: String,
    pub registered: bool,
    pub email: String,
    pub institution: Option<String>,
    pub is_active: bool,
    pub can_contribute: bool,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = md_user)]
pub struct NewUser {
    pub password: String,
    pub is_superuser: bool,
    pub username: String,
    pub is_staff: bool,
    pub date_joined: DateTime<Utc>,
    pub first_name: String,
    pub last_name: String,
    pub registered: bool,
    pub email: String,
    pub institution: Option<String>,
    pub is_active: bool,
    pub can_contribute: bool,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = md_user)]
pub struct UserUpdate {
    pub password: Option<String>,
    pub is_superuser: Option<bool>,
    pub username: Option<String>,
    pub is_staff: Option<bool>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub registered: Option<bool>,
    pub email: Option<String>,
    pub institution: Option<Option<String>>,
    pub is_active: Option<bool>,
    pub can_contribute: Option<bool>,
}

// ── socialaccount_socialaccount ────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = socialaccount_socialaccount)]
#[diesel(belongs_to(User))]
pub struct SocialAccount {
    pub id: i32,
    pub provider: String,
    pub uid: String,
    pub last_login: DateTime<Utc>,
    pub date_joined: DateTime<Utc>,
    pub extra_data: String,
    pub user_id: i64,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = socialaccount_socialaccount)]
pub struct NewSocialAccount {
    pub provider: String,
    pub uid: String,
    pub last_login: DateTime<Utc>,
    pub date_joined: DateTime<Utc>,
    pub extra_data: String,
    pub user_id: i64,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = socialaccount_socialaccount)]
pub struct SocialAccountUpdate {
    pub provider: Option<String>,
    pub uid: Option<String>,
    pub last_login: Option<DateTime<Utc>>,
    pub date_joined: Option<DateTime<Utc>>,
    pub extra_data: Option<String>,
    pub user_id: Option<i64>,
}

// ── socialaccount_socialapp ────────────────────────────────────────────────────

#[derive(
    Debug, Queryable, Selectable, Identifiable, Serialize, Deserialize, ToSchema,
)]
#[diesel(table_name = socialaccount_socialapp)]
pub struct SocialApp {
    pub id: i32,
    pub provider: String,
    pub name: String,
    pub client_id: String,
    pub secret: String,
    pub key: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = socialaccount_socialapp)]
pub struct NewSocialApp {
    pub provider: String,
    pub name: String,
    pub client_id: String,
    pub secret: String,
    pub key: String,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = socialaccount_socialapp)]
pub struct SocialAppUpdate {
    pub provider: Option<String>,
    pub name: Option<String>,
    pub client_id: Option<String>,
    pub secret: Option<String>,
    pub key: Option<String>,
}

// ── socialaccount_socialapp_sites ──────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = socialaccount_socialapp_sites)]
#[diesel(belongs_to(SocialApp, foreign_key = socialapp_id))]
pub struct SocialAppSite {
    pub id: i64,
    pub socialapp_id: i32,
    pub site_id: i32,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = socialaccount_socialapp_sites)]
pub struct NewSocialAppSite {
    pub socialapp_id: i32,
    pub site_id: i32,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = socialaccount_socialapp_sites)]
pub struct SocialAppSiteUpdate {
    pub socialapp_id: Option<i32>,
    pub site_id: Option<i32>,
}

// ── socialaccount_socialtoken ──────────────────────────────────────────────────

#[derive(
    Debug,
    Queryable,
    Selectable,
    Identifiable,
    Associations,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[diesel(table_name = socialaccount_socialtoken)]
#[diesel(belongs_to(SocialAccount, foreign_key = account_id))]
#[diesel(belongs_to(SocialApp, foreign_key = app_id))]
pub struct SocialToken {
    pub id: i32,
    pub token: String,
    pub token_secret: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub account_id: i32,
    pub app_id: i32,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = socialaccount_socialtoken)]
pub struct NewSocialToken {
    pub token: String,
    pub token_secret: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub account_id: i32,
    pub app_id: i32,
}

#[derive(Debug, AsChangeset, Default, Deserialize)]
#[diesel(table_name = socialaccount_socialtoken)]
pub struct SocialTokenUpdate {
    pub token: Option<String>,
    pub token_secret: Option<String>,
    pub expires_at: Option<Option<DateTime<Utc>>>,
    pub account_id: Option<i32>,
    pub app_id: Option<i32>,
}
