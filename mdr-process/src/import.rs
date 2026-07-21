//! In-process database import — the Rust replacement for the
//! `import_preprocessed.py` shell-out.
//!
//! Takes the same payload `make_import_json` writes to `import.json` and writes
//! the simulation and all of its related rows through the `mdr-db` diesel layer.
//! Everything happens inside **one transaction**, so a failure leaves no
//! half-imported simulation behind (the script ran with `autocommit = True`).
//!
//! Each step mirrors one of the script's `create_*` helpers: find by natural
//! key, then update that row or insert a new one.
//!
//! Deliberate divergences from the Python (all of them narrow):
//!
//! - **Missing text is NULL, not `""`.** The script defaults absent
//!   `water_type` / `forcefield` / `forcefield_comments` and absent contributor
//!   fields to the empty string; every one of those columns is nullable, so we
//!   leave them NULL rather than storing two spellings of "unknown".
//! - **Software with no version dedups** — see
//!   [`ops::find_software_id_by_name_version`].
//! - **Warnings are not written.** They reach `md_upload_instance_message` via
//!   `ticket.rs`; the redundant `md_import_warning` copy is gone.
//! - **New publications keep their `pages` and `number`.** The script's INSERT
//!   listed only `(title, authors, journal, volume, year, doi)`, dropping two
//!   fields the payload carries. An *existing* pub is still reused untouched,
//!   exactly as the script did, so this only affects rows we create.
//! - **A bare `Cl` solute becomes `Cl-`, not the script's `Cl+`.** Chloride is an
//!   anion; the script wrote a cation that does not exist in these systems.

use crate::types::{ExportSimulation, MdFile};
use anyhow::{anyhow, Result};
use chrono::Utc;
use diesel::{connection::Connection, PgConnection};
use libmdrepo::metadata;
use log::debug;
use mdr_db::{
    models::*,
    ops::{self, ContributionKey},
};
use std::path::Path;

/// Uploads made under this ORCID belong to the MDRepo admins, not to a user, so
/// the simulation is left with no `created_by_id`.
const ADMIN_ORCID: &str = "0000-0000-0000-0000";

/// Solute names the frontend expects in their ionic form. Chloride is an anion:
/// the script's table said `Cl+`, which is why we diverge from it here.
const SOLUTE_RENAMES: [(&str, &str); 3] = [("Na", "Na+"), ("Cl", "Cl-"), ("K", "K+")];

// --------------------------------------------------
#[derive(Debug, Default)]
pub struct ImportOpts {
    /// Reprocessing an existing simulation: its previous processed files are
    /// deleted before the new ones are written.
    pub reprocess_simulation_id: Option<u64>,

    /// Also delete the previous *uploaded* (original) files.
    pub replace_original_files: bool,
}

// --------------------------------------------------
/// Import a processed simulation, returning its `md_simulation.id`. All writes
/// share one transaction and roll back together on any error.
pub fn import_simulation(
    conn: &mut PgConnection,
    sim: &ExportSimulation,
    opts: &ImportOpts,
) -> Result<i64> {
    conn.transaction(|conn| {
        let sim_id = upsert_simulation(conn, sim, opts)?;
        let mdrepo_id = format!("MDR{sim_id:08}");
        debug!("Importing {mdrepo_id}");

        if opts.reprocess_simulation_id.is_some() {
            let n = ops::delete_processed_files_for_simulation(conn, sim_id)?;
            debug!("Removed {n} previous processed file(s)");

            if opts.replace_original_files {
                let n = ops::delete_uploaded_files_for_simulation(conn, sim_id)?;
                debug!("Removed {n} previous uploaded file(s)");
            }
        }

        for file in &sim.original_files {
            upsert_uploaded_file(conn, sim_id, &mdrepo_id, file)?;
        }
        for file in &sim.processed_files {
            upsert_processed_file(conn, sim_id, &mdrepo_id, file)?;
        }
        for trajectory in &sim.replicates {
            upsert_replicate(conn, sim_id, trajectory)?;
        }
        for (rank, contributor) in sim.contributors.iter().enumerate() {
            upsert_contributor(conn, sim_id, contributor, rank as i32 + 1)?;
        }
        for ligand in &sim.ligands {
            upsert_ligand(conn, sim_id, ligand)?;
        }
        for solute in &sim.solutes {
            upsert_solute(conn, sim_id, solute)?;
        }
        for link in &sim.external_links {
            upsert_external_link(conn, sim_id, link)?;
        }
        for paper in &sim.papers {
            upsert_paper(conn, sim_id, paper)?;
        }
        for uniprot in &sim.uniprots {
            upsert_uniprot(conn, sim_id, uniprot)?;
        }
        if let Some(pdb) = &sim.pdb {
            upsert_pdb(conn, sim_id, pdb)?;
        }

        Ok(sim_id)
    })
}

// --------------------------------------------------
/// Find the simulation this payload belongs to — by explicit id, then by
/// `(alias, creator)`, then by file hash — and update it; insert a new one when
/// nothing matches. Mirrors the script's `get_simulation`.
fn upsert_simulation(
    conn: &mut PgConnection,
    sim: &ExportSimulation,
    opts: &ImportOpts,
) -> Result<i64> {
    let user_id = lead_contributor_id(conn, &sim.lead_contributor_orcid)?;
    let software_id = find_or_create_software(conn, sim)?;

    let mut sim_id = opts.reprocess_simulation_id.map(|id| id as i64);

    if sim_id.is_none()
        && let Some(given) = sim.simulation_id
    {
        let given = given as i64;
        ops::get_simulation(conn, given)
            .map_err(|e| anyhow!("Invalid simulation ID {given}: {e}"))?;
        sim_id = Some(given);
    }

    if sim_id.is_none()
        && let Some(alias) = &sim.alias
    {
        sim_id = ops::find_simulation_id_by_alias(conn, alias, user_id)?;
    }

    if sim_id.is_none() {
        debug!("Searching unique_file_hash_string");
        sim_id = ops::find_simulation_id_by_hash(conn, &sim.unique_file_hash_string)?;
    }

    // The script hard-codes both of these on every import: a freshly imported
    // simulation is a placeholder until it is curated, and is never deprecated.
    let is_placeholder = true;
    let is_deprecated = false;

    if let Some(sim_id) = sim_id {
        ops::update_simulation(
            conn,
            sim_id,
            SimulationUpdate {
                software_id: Some(Some(software_id)),
                created_by_id: Some(user_id),
                unique_file_hash_string: Some(Some(
                    sim.unique_file_hash_string.clone(),
                )),
                alias: Some(sim.alias.clone()),
                description: Some(sim.description.clone()),
                short_description: Some(sim.short_description.clone()),
                run_commands: Some(sim.run_commands.clone()),
                duration: Some(Some(sim.duration)),
                sampling_frequency: Some(Some(sampling_frequency(sim))),
                integration_timestep_fs: Some(Some(sim.integration_timestep_fs as i32)),
                water_type: Some(sim.water_type.clone()),
                water_density: Some(sim.water_density),
                rmsd_values: Some(Some(sim.rmsd_values.clone())),
                rmsf_values: Some(Some(sim.rmsf_values.clone())),
                forcefield: Some(sim.forcefield.clone()),
                forcefield_comments: Some(sim.forcefield_comments.clone()),
                fasta_sequence: Some(Some(sim.fasta_sequence.clone())),
                num_replicates: Some(Some(sim.num_replicates as i32)),
                temperature: Some(Some(sim.temperature_kelvin as i32)),
                protonation_method: Some(sim.protonation_method.clone()),
                is_placeholder: Some(is_placeholder),
                is_deprecated: Some(is_deprecated),
                is_embargoed: Some(sim.is_embargoed.unwrap_or(false)),
                is_coarse_grained: Some(sim.is_coarse_grained.unwrap_or(false)),
                ..Default::default()
            },
        )?;
        return Ok(sim_id);
    }

    // A new simulation starts private; curation makes it public.
    let created = ops::insert_simulation(
        conn,
        NewSimulation {
            software_id: Some(software_id),
            created_by_id: user_id,
            unique_file_hash_string: Some(sim.unique_file_hash_string.clone()),
            alias: sim.alias.clone(),
            description: sim.description.clone(),
            short_description: sim.short_description.clone(),
            run_commands: sim.run_commands.clone(),
            duration: Some(sim.duration),
            sampling_frequency: Some(sampling_frequency(sim)),
            integration_timestep_fs: Some(sim.integration_timestep_fs as i32),
            water_type: sim.water_type.clone(),
            water_density: sim.water_density,
            rmsd_values: Some(sim.rmsd_values.clone()),
            rmsf_values: Some(sim.rmsf_values.clone()),
            forcefield: sim.forcefield.clone(),
            forcefield_comments: sim.forcefield_comments.clone(),
            fasta_sequence: Some(sim.fasta_sequence.clone()),
            num_replicates: Some(sim.num_replicates as i32),
            temperature: Some(sim.temperature_kelvin as i32),
            protonation_method: sim.protonation_method.clone(),
            is_placeholder,
            is_deprecated,
            is_public: false,
            is_embargoed: sim.is_embargoed.unwrap_or(false),
            is_coarse_grained: sim.is_coarse_grained.unwrap_or(false),
            creation_date: Utc::now(),
            pdb_id: None,
            irods_ticket: None,
            superseding_simulation_id: None,
        },
    )?;

    Ok(created.id)
}

// --------------------------------------------------
/// `md_simulation.sampling_frequency` is a double, but the payload carries an
/// `f32`. Go through the shortest round-trip decimal (what the JSON held) so the
/// stored value reads as `0.1`, not `0.10000000149011612`.
fn sampling_frequency(sim: &ExportSimulation) -> f64 {
    sim.sampling_frequency
        .to_string()
        .parse()
        .unwrap_or(sim.sampling_frequency as f64)
}

// --------------------------------------------------
/// The owning user for an upload, looked up from the lead contributor's ORCID.
/// Admin uploads have no owner; any other unknown ORCID is fatal.
fn lead_contributor_id(conn: &mut PgConnection, orcid: &str) -> Result<Option<i64>> {
    if orcid == ADMIN_ORCID {
        return Ok(None);
    }

    ops::find_user_id_by_orcid(conn, orcid)?
        .ok_or_else(|| anyhow!("Failed to find ORCID '{orcid}'"))
        .map(Some)
}

// --------------------------------------------------
fn find_or_create_software(
    conn: &mut PgConnection,
    sim: &ExportSimulation,
) -> Result<i64> {
    let version = Some(sim.software_version.as_str());
    if let Some(id) =
        ops::find_software_id_by_name_version(conn, &sim.software_name, version)?
    {
        return Ok(id);
    }

    Ok(ops::insert_software(
        conn,
        NewSoftware {
            name: sim.software_name.clone(),
            version: version.map(String::from),
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_uploaded_file(
    conn: &mut PgConnection,
    sim_id: i64,
    mdrepo_id: &str,
    file: &MdFile,
) -> Result<i64> {
    let local_file_path = format!("{mdrepo_id}/original/{}", file.name);
    let is_primary = file.is_primary.unwrap_or(false);

    if let Some(file_id) = ops::find_uploaded_file_id(conn, sim_id, &file.name)? {
        ops::update_uploaded_file(
            conn,
            file_id,
            UploadedFileUpdate {
                file_type: Some(file.file_type.clone()),
                description: Some(file.description.clone()),
                local_file_path: Some(local_file_path),
                md5_hash: Some(Some(file.md5_sum.clone())),
                file_size_bytes: Some(Some(file.size as i64)),
                is_primary: Some(is_primary),
                ..Default::default()
            },
        )?;
        return Ok(file_id);
    }

    Ok(ops::insert_uploaded_file(
        conn,
        NewUploadedFile {
            filename: file.name.clone(),
            file_type: file.file_type.clone(),
            simulation_id: sim_id,
            description: file.description.clone(),
            local_file_path,
            file_size_bytes: Some(file.size as i64),
            md5_hash: Some(file.md5_sum.clone()),
            is_primary,
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_processed_file(
    conn: &mut PgConnection,
    sim_id: i64,
    mdrepo_id: &str,
    file: &MdFile,
) -> Result<i64> {
    // Processed files are recorded under their basename — the payload names them
    // by their path under the processed directory.
    let filename = Path::new(&file.name)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| file.name.clone());
    let local_file_path = format!("{mdrepo_id}/processed/{filename}");

    if let Some(file_id) = ops::find_processed_file_id(conn, sim_id, &filename)? {
        ops::update_processed_file(
            conn,
            file_id,
            ProcessedFileUpdate {
                file_type: Some(file.file_type.clone()),
                description: Some(file.description.clone()),
                local_file_path: Some(local_file_path),
                md5_hash: Some(Some(file.md5_sum.clone())),
                file_size_bytes: Some(Some(file.size as i64)),
                ..Default::default()
            },
        )?;
        return Ok(file_id);
    }

    Ok(ops::insert_processed_file(
        conn,
        NewProcessedFile {
            file_type: file.file_type.clone(),
            local_file_path,
            filename,
            simulation_id: sim_id,
            file_size_bytes: Some(file.size as i64),
            description: file.description.clone(),
            md5_hash: Some(file.md5_sum.clone()),
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_replicate(
    conn: &mut PgConnection,
    sim_id: i64,
    trajectory: &str,
) -> Result<i64> {
    if let Some(id) = ops::find_replicate_id(conn, sim_id, trajectory)? {
        return Ok(id);
    }

    Ok(ops::insert_replicate(
        conn,
        NewReplicate {
            trajectory_file_name: trajectory.to_string(),
            simulation_id: sim_id,
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_contributor(
    conn: &mut PgConnection,
    sim_id: i64,
    contributor: &metadata::Contributor,
    rank: i32,
) -> Result<i64> {
    // ORCID identifies a contributor best, then email, then the name they gave.
    let key = match (&contributor.orcid, &contributor.email) {
        (Some(orcid), _) => ContributionKey::Orcid(orcid),
        (None, Some(email)) => ContributionKey::Email(email),
        (None, None) => ContributionKey::Name(&contributor.name),
    };

    if let Some(id) = ops::find_contribution_id(conn, sim_id, key)? {
        ops::update_contribution(
            conn,
            id,
            ContributionUpdate {
                orcid: Some(contributor.orcid.clone()),
                name: Some(Some(contributor.name.clone())),
                email: Some(contributor.email.clone()),
                institution: Some(contributor.institution.clone()),
                rank: Some(rank),
                ..Default::default()
            },
        )?;
        return Ok(id);
    }

    Ok(ops::insert_contribution(
        conn,
        NewContribution {
            email: contributor.email.clone(),
            institution: contributor.institution.clone(),
            name: Some(contributor.name.clone()),
            orcid: contributor.orcid.clone(),
            simulation_id: Some(sim_id),
            rank,
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_ligand(
    conn: &mut PgConnection,
    sim_id: i64,
    ligand: &metadata::Ligand,
) -> Result<i64> {
    if let Some(id) = ops::find_ligand_id(conn, sim_id, &ligand.name)? {
        ops::update_ligand(
            conn,
            id,
            LigandUpdate {
                smiles_string: Some(ligand.smiles.clone()),
                ..Default::default()
            },
        )?;
        return Ok(id);
    }

    Ok(ops::insert_ligand(
        conn,
        NewLigand {
            name: ligand.name.clone(),
            smiles_string: ligand.smiles.clone(),
            simulation_id: sim_id,
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_solute(
    conn: &mut PgConnection,
    sim_id: i64,
    solute: &metadata::Solute,
) -> Result<i64> {
    let name = SOLUTE_RENAMES
        .iter()
        .find(|(from, _)| *from == solute.name)
        .map_or(solute.name.as_str(), |(_, to)| to);

    if let Some(id) = ops::find_solute_id(conn, sim_id, name)? {
        ops::update_solute(
            conn,
            id,
            SoluteUpdate {
                concentration: Some(solute.concentration_mol_liter),
                ..Default::default()
            },
        )?;
        return Ok(id);
    }

    Ok(ops::insert_solute(
        conn,
        NewSolute {
            name: name.to_string(),
            concentration: solute.concentration_mol_liter,
            simulation_id: sim_id,
        },
    )?
    .id)
}

// --------------------------------------------------
fn upsert_external_link(
    conn: &mut PgConnection,
    sim_id: i64,
    link: &metadata::ExternalLink,
) -> Result<i64> {
    if let Some(id) = ops::find_external_link_id(conn, sim_id, &link.url)? {
        // Only overwrite the label when this payload has one, so a curated label
        // survives a reprocess.
        if link.label.is_some() {
            ops::update_external_link(
                conn,
                id,
                ExternalLinkUpdate {
                    label: Some(link.label.clone()),
                    ..Default::default()
                },
            )?;
        }
        return Ok(id);
    }

    Ok(ops::insert_external_link(
        conn,
        NewExternalLink {
            url: link.url.clone(),
            label: link.label.clone(),
            simulation_id: sim_id,
        },
    )?
    .id)
}

// --------------------------------------------------
/// Find or create the publication, then link it to the simulation. Papers are
/// shared across simulations, so an existing pub is reused as-is.
fn upsert_paper(
    conn: &mut PgConnection,
    sim_id: i64,
    paper: &metadata::Paper,
) -> Result<i64> {
    let volume = paper.volume as i32;
    let year = paper.year as i32;

    let existing = match &paper.doi {
        Some(doi) => ops::find_pub_id_by_doi(conn, doi)?,
        None => ops::find_pub_id_by_metadata(
            conn,
            &paper.title,
            &paper.authors,
            &paper.journal,
            volume,
            year,
        )?,
    };

    let pub_id = match existing {
        Some(id) => id,
        None => {
            ops::insert_pub(
                conn,
                NewPub {
                    title: paper.title.clone(),
                    authors: paper.authors.clone(),
                    journal: paper.journal.clone(),
                    volume,
                    number: paper.number.clone(),
                    year,
                    pages: paper.pages.clone(),
                    doi: paper.doi.clone(),
                },
            )?
            .id
        }
    };

    if let Some(id) = ops::find_simulation_pub_id(conn, sim_id, pub_id)? {
        return Ok(id);
    }

    Ok(ops::insert_simulation_pub(
        conn,
        NewSimulationPub {
            simulation_id: sim_id,
            pub_id,
        },
    )?
    .id)
}

// --------------------------------------------------
/// Find or create the UniProt entry (shared across simulations), refresh it from
/// this payload, then link it to the simulation.
fn upsert_uniprot(
    conn: &mut PgConnection,
    sim_id: i64,
    uniprot: &crate::types::UniprotEntry,
) -> Result<i64> {
    let amino_length = uniprot.sequence.len() as i32;

    let uniprot_pk =
        match ops::find_uniprot_id_by_accession(conn, &uniprot.uniprot_id)? {
            Some(id) => {
                ops::update_uniprot(
                    conn,
                    id,
                    UniprotUpdate {
                        name: Some(uniprot.name.clone()),
                        amino_length: Some(amino_length),
                        sequence: Some(uniprot.sequence.clone()),
                        ..Default::default()
                    },
                )?;
                id
            }
            None => {
                ops::insert_uniprot(
                    conn,
                    NewUniprot {
                        uniprot_id: uniprot.uniprot_id.clone(),
                        name: uniprot.name.clone(),
                        amino_length,
                        sequence: uniprot.sequence.clone(),
                    },
                )?
                .id
            }
        };

    if let Some(id) = ops::find_simulation_uniprot_id(conn, sim_id, uniprot_pk)? {
        return Ok(id);
    }

    Ok(ops::insert_simulation_uniprot(
        conn,
        NewSimulationUniprot {
            simulation_id: sim_id,
            uniprot_id: uniprot_pk,
        },
    )?
    .id)
}

// --------------------------------------------------
/// Find or create the PDB entry and point the simulation at it. Returns the
/// `md_pdb.id`.
fn upsert_pdb(
    conn: &mut PgConnection,
    sim_id: i64,
    pdb: &crate::types::PdbEntry,
) -> Result<i64> {
    let code = pdb.pdb_id.to_lowercase();

    let pdb_pk = match ops::find_pdb_id_by_code(conn, &code)? {
        Some(id) => {
            ops::update_pdb(
                conn,
                id,
                PdbUpdate {
                    title: Some(Some(pdb.title.clone())),
                    classification: Some(Some(pdb.classification.clone())),
                    ..Default::default()
                },
            )?;
            id
        }
        None => {
            ops::insert_pdb(
                conn,
                NewPdb {
                    pdb_id: code,
                    classification: Some(pdb.classification.clone()),
                    title: Some(pdb.title.clone()),
                },
            )?
            .id
        }
    };

    ops::update_simulation(
        conn,
        sim_id,
        SimulationUpdate {
            pdb_id: Some(Some(pdb_pk)),
            ..Default::default()
        },
    )?;

    Ok(pdb_pk)
}
