//! In-process database import — the Rust replacement for the
//! `import_preprocessed.py` shell-out.
//!
//! Takes the same payload `make_import_json` writes to `import.json` and writes
//! the simulation and all of its related rows through the `mdr-db` diesel layer.
//! Everything happens inside **one transaction**, so a failure leaves no
//! half-imported simulation behind (the script ran with `autocommit = True`).
//!
//! The one deliberate exception is the `md_software` lookup, which is resolved
//! *before* that transaction opens -- see [`import_simulation`].
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

use crate::types::{
    ExportSimulation, ImportChain, MdFile, PolymerLookup, ResolvedLigand,
};
use anyhow::{Result, anyhow, bail};
use chrono::Utc;
use diesel::{PgConnection, connection::Connection};
use libmdrepo::metadata;
use log::debug;
use mdr_db::{models::*, ops};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path};

/// Uploads made under this ORCID are attributed to the fixed `mdrepo_admin`
/// account rather than a specific contributor.
const ADMIN_ORCID: &str = "0000-0000-0000-0000";
const ADMIN_USERNAME: &str = "mdrepo_admin";

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

    /// The upload ticket this simulation belongs to, stored on the row so the
    /// ticket view can count its simulations. `None` for runs with no ticket
    /// (e.g. a direct reprocess), which then leaves any existing link intact.
    pub ticket_id: Option<i64>,
}

// --------------------------------------------------
/// Import a processed simulation, returning its `md_simulation.id`. All writes
/// share one transaction and roll back together on any error -- except the
/// `md_software` lookup, which is resolved first, outside it.
///
/// That lookup is hoisted because `md_software` is a small shared lookup table,
/// not part of this simulation's atomic unit, and every importer of a given
/// software touches the same row. Resolved inside the transaction, the row lock
/// it takes was held until the whole import committed (~60-75 s), so concurrent
/// importers of the same software ran strictly one at a time -- the 2026-08-22
/// serialisation, where `--ingest-jobs 8` had an effective concurrency of 1.
/// Out here the lock lives for one statement.
///
/// The trade: a *newly created* software row now survives a failed import,
/// where before it rolled back. That is harmless -- an unreferenced lookup row,
/// which the retry reuses -- and is why this is the only step outside.
pub fn import_simulation(
    conn: &mut PgConnection,
    sim: &ExportSimulation,
    opts: &ImportOpts,
) -> Result<i64> {
    let software_id = find_or_create_software(conn, sim)?;

    conn.transaction(|conn| {
        let sim_id = upsert_simulation(conn, sim, opts, software_id)?;
        let mdrepo_id = format!("MDR{sim_id:08}");
        debug!("Importing {mdrepo_id}");

        if opts.reprocess_simulation_id.is_some() {
            let n = ops::delete_processed_files_for_simulation(conn, sim_id)?;
            debug!("Removed {n} previous processed file(s)");

            if opts.replace_original_files {
                let n = ops::delete_uploaded_files_for_simulation(conn, sim_id)?;
                debug!("Removed {n} previous uploaded file(s)");
            }

            // Not gated on `replace_original_files`: both of these are
            // find-or-insert below, so leaving them would keep rows the new
            // metadata no longer names. `md_replicate` is what mdr-export
            // publishes `trajectory_file_names` from, so a stale row there is
            // visible, not just stored.
            let n = ops::delete_replicates_for_simulation(conn, sim_id)?;
            debug!("Removed {n} previous replicate(s)");

            let n = ops::delete_uniprots_for_simulation(conn, sim_id)?;
            debug!("Removed {n} previous uniprot link(s)");
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
        for (rank, creator) in sim.creators.iter().enumerate() {
            upsert_creator(conn, sim_id, creator, rank as i32 + 1)?;
        }
        // Chains are replaced on every import, not only a reprocess: they
        // are measured from this run's full.pdb, never merged with an earlier
        // run's. The polymer ligands pointing at them go first.
        let n = ops::delete_chains_for_simulation(conn, sim_id)?;
        if n > 0 {
            debug!("Removed {n} previous chain(s)");
        }
        let mut chain_ids = HashMap::new();
        for chain in &sim.chains {
            chain_ids.insert(chain.chain_order, insert_chain(conn, sim_id, chain)?);
        }
        for ligand in &sim.ligands {
            upsert_ligand(conn, sim_id, ligand, &chain_ids)?;
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
        if !sim.collections.is_empty() {
            let owner_user_id = lead_contributor_id(conn, &sim.lead_contributor_orcid)?
                .ok_or_else(|| {
                    anyhow!("collections require a resolvable lead_contributor")
                })?;
            for name in &sim.collections {
                upsert_collection(conn, sim_id, owner_user_id, name)?;
            }
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
    software_id: i64,
) -> Result<i64> {
    let user_id = lead_contributor_id(conn, &sim.lead_contributor_orcid)?;

    let mut sim_id = opts.reprocess_simulation_id.map(|id| id as i64);
    let mut matched_by = "--reprocess-simulation-id";

    if sim_id.is_none()
        && let Some(given) = sim.simulation_id
    {
        let given = given as i64;
        ops::get_simulation(conn, given)
            .map_err(|e| anyhow!("Invalid simulation ID {given}: {e}"))?;
        sim_id = Some(given);
        matched_by = "its simulation_id";
    }

    if sim_id.is_none()
        && let Some(alias) = &sim.alias
    {
        sim_id = ops::find_simulation_id_by_alias(conn, alias, user_id)?;
        matched_by = "alias and lead contributor";
    }

    if sim_id.is_none() {
        debug!("Searching unique_file_hash_string");
        sim_id = ops::find_simulation_id_by_hash(conn, &sim.unique_file_hash_string)?;
        matched_by = "unique_file_hash_string";
    }

    // Only a reprocess clears the old file, replicate and uniprot rows (see
    // import_simulation), so updating a finished simulation any other way
    // leaves the rows the new payload no longer names beside the new ones
    // (MDR-74). Whether that is a real supersede, and whether the original
    // files go too, is an operator's call. A placeholder is let through: it
    // is this landing's own earlier attempt, which ticket.rs retries until a
    // verified push clears the flag.
    if let Some(existing) = sim_id
        && opts.reprocess_simulation_id.is_none()
        && !ops::get_simulation(conn, existing)?.is_placeholder
    {
        bail!(
            "This upload matches existing simulation MDR{existing:08} by \
            {matched_by}. Replacing it needs an administrator: \
            `mdr-process process --reprocess-simulation-id {existing}` \
            (add --replace-original-files to drop its old uploaded files)"
        );
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
                contributor_id: Some(user_id),
                unique_file_hash_string: Some(Some(
                    sim.unique_file_hash_string.clone(),
                )),
                alias: Some(sim.alias.clone()),
                description: Some(sim.description.clone()),
                short_description: Some(sim.short_description.clone()),
                run_commands: Some(sim.run_commands.clone()),
                duration: Some(Some(sim.duration)),
                sampling_frequency: Some(Some(sampling_frequency(sim))),
                sampling_frequency_ps: Some(sim.sampling_frequency_ps),
                integration_timestep_fs: Some(Some(sim.integration_timestep_fs as i32)),
                water_type: Some(sim.water_type.clone()),
                water_density: Some(sim.water_density),
                rmsd_values: Some(Some(sim.rmsd_values.clone())),
                rmsf_values: Some(Some(sim.rmsf_values.clone())),
                forcefield: Some(sim.forcefield.clone()),
                forcefield_comments: Some(sim.forcefield_comments.clone()),
                // Empty for a DNA- or RNA-only structure: no protein chains
                fasta_sequence: Some(non_empty(&sim.fasta_sequence)),
                num_replicates: Some(Some(sim.num_replicates as i32)),
                temperature: Some(Some(sim.temperature_kelvin as i32)),
                protonation_method: Some(sim.protonation_method.clone()),
                is_placeholder: Some(is_placeholder),
                is_deprecated: Some(is_deprecated),
                is_embargoed: Some(sim.is_embargoed.unwrap_or(false)),
                is_coarse_grained: Some(sim.is_coarse_grained.unwrap_or(false)),
                // Only overwrite the ticket link when this run has one; a
                // ticket-less reprocess leaves the existing value untouched.
                md_repo_ticket_id: opts.ticket_id.map(Some),
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
            contributor_id: user_id,
            unique_file_hash_string: Some(sim.unique_file_hash_string.clone()),
            alias: sim.alias.clone(),
            description: sim.description.clone(),
            short_description: sim.short_description.clone(),
            run_commands: sim.run_commands.clone(),
            duration: Some(sim.duration),
            sampling_frequency: Some(sampling_frequency(sim)),
            sampling_frequency_ps: sim.sampling_frequency_ps,
            integration_timestep_fs: Some(sim.integration_timestep_fs as i32),
            water_type: sim.water_type.clone(),
            water_density: sim.water_density,
            rmsd_values: Some(sim.rmsd_values.clone()),
            rmsf_values: Some(sim.rmsf_values.clone()),
            forcefield: sim.forcefield.clone(),
            forcefield_comments: sim.forcefield_comments.clone(),
            fasta_sequence: non_empty(&sim.fasta_sequence),
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
            md_repo_ticket_id: opts.ticket_id,
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
/// Admin uploads resolve to the fixed `mdrepo_admin` account; any other
/// unknown ORCID is fatal.
fn lead_contributor_id(conn: &mut PgConnection, orcid: &str) -> Result<Option<i64>> {
    if orcid == ADMIN_ORCID {
        return ops::find_user_id_by_username(conn, ADMIN_USERNAME)?
            .ok_or_else(|| anyhow!("Admin user '{ADMIN_USERNAME}' not found"))
            .map(Some);
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

    // One statement, not find-then-insert -- see ops::upsert_software. This is
    // the race that had already fired: with no unique constraint on
    // md_software the loser used to insert a duplicate silently. The
    // constraint arrived in Django migration 0258, which this depends on.
    Ok(ops::upsert_software(
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
/// Write one creator of a simulation: the identity into `md_creator`,
/// the link and its author rank into `md_simulation_creator`.
///
/// Values are stored exactly as submitted. The lower-casing lives only in the
/// match, never in what is written, so the first spelling of a person to
/// arrive stays the canonical row and a later variant links to it rather than
/// rewriting it.
fn upsert_creator(
    conn: &mut PgConnection,
    sim_id: i64,
    creator: &metadata::Creator,
    rank: i32,
) -> Result<i64> {
    let creator_id = ops::upsert_creator(
        conn,
        NewCreator {
            name: Some(creator.name.clone()),
            orcid: creator.orcid.clone(),
            email: creator.email.clone(),
            institution: creator.institution.clone(),
        },
    )?;

    ops::upsert_simulation_creator(
        conn,
        NewSimulationCreator {
            simulation_id: sim_id,
            creator_id,
            rank,
        },
    )?;

    Ok(creator_id)
}

// --------------------------------------------------
/// Write one ligand exactly as `resolve_ligands` decided it.
///
/// Deliberately no derivation and no judgement here. Every field including
/// `declared_identity` was settled in `process::resolve_ligands`, which is the
/// only place that still knows whether the submitter declared these ligands or
/// whether they were perceived from coordinates. Recomputing any of it from
/// the resolved values would answer confidently and wrongly.
///
/// `update` names all six columns rather than merging, so a reprocess of a
/// simulation whose ligands changed cannot leave the previous run's InChI
/// beside the new run's SMILES.
fn upsert_ligand(
    conn: &mut PgConnection,
    sim_id: i64,
    ligand: &ResolvedLigand,
    chain_ids: &HashMap<u32, i64>,
) -> Result<i64> {
    let existing = ops::find_ligand_id(conn, sim_id, &ligand.name)?;

    // A polymer ligand: a new row pointing at its chain. Any row of the same
    // name left from an earlier import goes, since a row is a SMILES or a
    // chain and an update cannot move it from one to the other.
    if let Some(order) = ligand.chain_order {
        let chain_id = *chain_ids.get(&order).ok_or_else(|| {
            anyhow!(
                r#"Ligand "{}" names chain {order}, which is not in the import"#,
                ligand.name
            )
        })?;
        if let Some(id) = existing {
            ops::delete_ligand(conn, id)?;
        }
        return Ok(ops::insert_ligand(
            conn,
            NewLigand {
                name: ligand.name.clone(),
                smiles: None,
                inchi: None,
                inchikey: None,
                declared_identity: ligand.declared_identity.clone(),
                identity_software: None,
                simulation_id: sim_id,
                chain_id: Some(chain_id),
            },
        )?
        .id);
    }

    let smiles = ligand.smiles.clone().ok_or_else(|| {
        anyhow!(
            r#"Ligand "{}" has neither a SMILES nor a chain"#,
            ligand.name
        )
    })?;

    if let Some(id) = existing {
        ops::update_ligand(
            conn,
            id,
            LigandUpdate {
                smiles: Some(smiles),
                inchi: ligand.inchi.clone(),
                inchikey: ligand.inchikey.clone(),
                declared_identity: ligand.declared_identity.clone(),
                identity_software: ligand.identity_software.clone(),
                ..Default::default()
            },
        )?;
        return Ok(id);
    }

    Ok(ops::insert_ligand(
        conn,
        NewLigand {
            name: ligand.name.clone(),
            smiles: Some(smiles),
            inchi: ligand.inchi.clone(),
            inchikey: ligand.inchikey.clone(),
            declared_identity: ligand.declared_identity.clone(),
            identity_software: ligand.identity_software.clone(),
            simulation_id: sim_id,
            chain_id: None,
        },
    )?
    .id)
}

// --------------------------------------------------
fn non_empty(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_string())
}

// --------------------------------------------------
/// `md_polymer.residues_hash`: the lower-case hex SHA-256 of the residue codes
/// joined by commas (`ALA,GLY,SER`). Any other writer of md_polymer (the
/// full.pdb backfill) must hash the same way, or one sequence gets two rows.
pub fn residues_hash(residues: &[String]) -> String {
    format!("{:x}", Sha256::digest(residues.join(",").as_bytes()))
}

// --------------------------------------------------
/// Insert one chain, and its polymer if the polymer is new, and record the
/// polymer's UniProt lookup if it has none yet. Returns the chain's id.
fn insert_chain(
    conn: &mut PgConnection,
    sim_id: i64,
    chain: &ImportChain,
) -> Result<i64> {
    let polymer_id = ops::find_or_insert_polymer(
        conn,
        &NewPolymer {
            polymer_type: chain.polymer_type.clone(),
            sequence: chain.sequence.clone(),
            residues: chain.residues.clone(),
            residues_hash: residues_hash(&chain.residues),
            num_residues: i32::try_from(chain.residues.len())?,
        },
    )?;

    if let Some(lookup) = &chain.reference {
        set_polymer_reference(conn, polymer_id, lookup)?;
    }

    Ok(ops::insert_chain(
        conn,
        NewChain {
            chain_order: i32::try_from(chain.chain_order)?,
            chain_label: chain.chain_label.clone(),
            source: chain.source.clone(),
            first_residue: chain.first_residue,
            last_residue: chain.last_residue,
            n_terminal_cap: chain.n_terminal_cap.clone(),
            c_terminal_cap: chain.c_terminal_cap.clone(),
            simulation_id: sim_id,
            polymer_id,
        },
    )?
    .id)
}

// --------------------------------------------------
/// Write a polymer's UniProt lookup, unless it was looked up before: a
/// polymer's reference is set once (`ops::set_polymer_reference`). A hit's
/// UniProt entry is upserted only when it will be written, and is not linked
/// to the simulation; md_simulation_uniprot keeps the TOML's accessions.
fn set_polymer_reference(
    conn: &mut PgConnection,
    polymer_id: i64,
    lookup: &PolymerLookup,
) -> Result<()> {
    let untried = ops::get_polymer(conn, polymer_id)?.match_method.is_none();
    if !untried {
        debug!("Polymer {polymer_id} was looked up before; keeping its reference");
        return Ok(());
    }
    let reference = match &lookup.hit {
        Some(hit) => Some(PolymerReference {
            match_method: lookup.match_method.clone(),
            uniprot_id: ops::upsert_uniprot(conn, new_uniprot(&hit.uniprot))?.id,
            query_start: hit.query_start,
            query_end: hit.query_end,
            reference_start: hit.reference_start,
            reference_end: hit.reference_end,
            identity: hit.identity,
        }),
        None => None,
    };
    ops::set_polymer_reference(conn, polymer_id, reference.as_ref())?;
    Ok(())
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

    let new_pub = || NewPub {
        title: paper.title.clone(),
        authors: paper.authors.clone(),
        journal: paper.journal.clone(),
        volume,
        number: paper.number.clone(),
        year,
        pages: paper.pages.clone(),
        doi: paper.doi.clone(),
    };

    // With a DOI the lookup is one race-safe statement; see
    // ops::upsert_pub_by_doi. Without one there is no unique key to conflict
    // on, so a concurrent first sighting can still insert a duplicate row
    // (MDR-37); it's rare, and it duplicates rather than fails.
    let pub_id = match &paper.doi {
        Some(_) => ops::upsert_pub_by_doi(conn, new_pub())?,
        None => match ops::find_pub_id_by_metadata(
            conn,
            &paper.title,
            &paper.authors,
            &paper.journal,
            volume,
            year,
        )? {
            Some(id) => id,
            None => ops::insert_pub(conn, new_pub())?.id,
        },
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
    // One statement, not find-then-insert: landing directories are processed in
    // parallel and md_uniprot is shared across simulations, so two threads
    // citing the same new accession used to both find nothing and both insert.
    // See ops::upsert_uniprot for the ticket 2175 failure this fixes. Both old
    // branches ended up writing these same three fields, so the behaviour for
    // an accession that already exists is unchanged.
    let uniprot_pk = ops::upsert_uniprot(conn, new_uniprot(uniprot))?.id;

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

/// An md_uniprot row from a fetched entry. A raw response left out (None)
/// keeps the stored one (`ops::upsert_uniprot`).
fn new_uniprot(uniprot: &crate::types::UniprotEntry) -> NewUniprot {
    NewUniprot {
        uniprot_id: uniprot.uniprot_id.clone(),
        name: uniprot.name.clone(),
        amino_length: uniprot.sequence.len() as i32,
        sequence: uniprot.sequence.clone(),
        response: uniprot.response.clone(),
        entry_version: uniprot.entry_version,
        fetched_at: uniprot.fetched_at,
    }
}

// --------------------------------------------------
/// Find or create the (owner, name) collection, then link it to the
/// simulation. Mirrors `upsert_uniprot`'s shape, but the identity row is
/// scoped per-owner rather than shared globally -- see `ops::upsert_collection`.
fn upsert_collection(
    conn: &mut PgConnection,
    sim_id: i64,
    owner_user_id: i64,
    name: &str,
) -> Result<i64> {
    let collection_pk = ops::upsert_collection(
        conn,
        NewCollection {
            user_id: owner_user_id,
            name: name.to_string(),
            description: None,
        },
    )?
    .id;

    if let Some(id) = ops::find_simulation_collection_id(conn, sim_id, collection_pk)? {
        return Ok(id);
    }

    Ok(ops::insert_simulation_collection(
        conn,
        NewSimulationCollection {
            simulation_id: sim_id,
            collection_id: collection_pk,
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

    // One statement, not find-then-insert -- see ops::upsert_pdb. Identical
    // race to the uniprot one: md_pdb is shared across simulations, landings
    // run in parallel, and UNIQUE (pdb_id) turns the collision into a failed
    // directory. Both old branches wrote title and classification from the
    // payload, so behaviour for a code that already exists is unchanged.
    let pdb_pk = ops::upsert_pdb(
        conn,
        NewPdb {
            pdb_id: code,
            classification: Some(pdb.classification.clone()),
            title: Some(pdb.title.clone()),
            response: pdb.response.clone(),
            entities_response: pdb.entities_response.clone(),
            fetched_at: pdb.fetched_at,
        },
    )?
    .id;

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
