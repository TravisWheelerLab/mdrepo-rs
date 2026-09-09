//! Integration tests for `mdr_process::import::import_simulation` -- the
//! in-process replacement for `import_preprocessed.py` that has been the live
//! import path since 2026-07-21 with no test coverage of its own (MDR-14).
//!
//! These exercise the orchestration in `import.rs` itself: the upsert helpers
//! (`upsert_uploaded_file`, `upsert_contributor`, ...) and the
//! reprocess/`--replace-original-files` delete-then-reimport flow. The lower-
//! level natural-key finders and `ops::upsert_*` primitives these call are
//! already covered by `mdr-db/tests/finders.rs`; what's untested before this
//! file is whether `import_simulation` wires them together correctly end to
//! end -- e.g. that re-importing the same payload updates rather than
//! duplicates every related row, not just the ones finders.rs checks in
//! isolation.
//!
//! Needs a Postgres loaded with the mdrepo schema, same as
//! `mdr-db/tests/finders.rs`. Run via `mdr-process/tests/with-testdb.sh`, or
//! point `TEST_DATABASE_URL` at a schema loaded from
//! `mdr-db/tests/fixtures/schema.sql` yourself. Each test opens its own
//! connection in a `begin_test_transaction()`, so nothing written here is
//! ever committed.
//!
//! **Not covered here:** the concurrent find-then-insert races fixed in
//! MDR-37 (`upsert_software`/`upsert_uniprot`/`upsert_pdb` racing two real
//! threads). Those were verified by hand against staging when fixed and
//! would need a commit-based connection (not the rolled-back-transaction
//! pattern used throughout this file, where a second connection's conflicting
//! insert would simply block on the first's un-committed lock) to automate
//! safely -- left as a follow-up.

use diesel::Connection as _;
use diesel::pg::PgConnection;
use libmdrepo::metadata;
use mdr_db::models::*;
use mdr_db::ops;
use mdr_process::import::{self, ImportOpts};
use mdr_process::types::{
    ExportSimulation, MdFile, PdbEntry, ResolvedLigand, UniprotEntry,
};

/// A connection whose work always rolls back, or `None` when the test DB
/// isn't configured (so the caller can skip). Mirrors
/// `mdr-db/tests/finders.rs::test_conn`.
fn test_conn() -> Option<PgConnection> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let mut conn = PgConnection::establish(&url).expect("connect to TEST_DATABASE_URL");
    conn.begin_test_transaction()
        .expect("begin rolled-back test transaction");
    Some(conn)
}

macro_rules! conn_or_skip {
    () => {
        match test_conn() {
            Some(c) => c,
            None => {
                eprintln!("skipping: TEST_DATABASE_URL is unset");
                return;
            }
        }
    };
}

fn seed_user_with_orcid(c: &mut PgConnection, key: &str, orcid: &str) -> i64 {
    let user_id = ops::insert_user(
        c,
        NewUser {
            password: "!".into(),
            is_superuser: false,
            username: format!("user-{key}"),
            is_staff: false,
            date_joined: chrono::Utc::now(),
            first_name: "Test".into(),
            last_name: "User".into(),
            registered: true,
            email: format!("{key}@example.org"),
            institution: None,
            is_active: true,
            can_contribute: true,
        },
    )
    .expect("insert user")
    .id;

    ops::insert_social_account(
        c,
        NewSocialAccount {
            provider: "orcid".into(),
            uid: orcid.into(),
            last_login: chrono::Utc::now(),
            date_joined: chrono::Utc::now(),
            extra_data: serde_json::json!({}),
            user_id,
        },
    )
    .expect("insert social account");

    user_id
}

/// A minimal-but-complete payload with no related entities; tests add what
/// they need with struct-update syntax.
fn base_sim(key: &str, orcid: &str) -> ExportSimulation {
    ExportSimulation {
        simulation_id: None,
        lead_contributor_orcid: orcid.to_string(),
        unique_file_hash_string: format!("hash-{key}"),
        alias: Some(format!("alias-{key}")),
        short_description: format!("test simulation {key}"),
        description: None,
        software_name: format!("TESTSOFT-{key}"),
        software_version: "1.0".to_string(),
        run_commands: None,
        pdb: None,
        uniprots: vec![],
        external_links: vec![],
        collections: vec![],
        forcefield: None,
        forcefield_comments: None,
        protonation_method: None,
        rmsd_values: vec![],
        rmsf_values: vec![],
        duration: 100.0,
        sampling_frequency: 0.1,
        sampling_frequency_ps: None,
        integration_timestep_fs: 2,
        temperature_kelvin: 300,
        fasta_sequence: String::new(),
        num_replicates: 1,
        water_type: None,
        water_density: None,
        structure_hash: format!("struct-hash-{key}"),
        contributors: vec![],
        original_files: vec![],
        processed_files: vec![],
        replicates: vec![],
        ligands: vec![],
        solutes: vec![],
        papers: vec![],
        is_embargoed: None,
        is_coarse_grained: None,
    }
}

fn md_file(name: &str, file_type: &str) -> MdFile {
    MdFile {
        name: name.to_string(),
        file_type: file_type.to_string(),
        size: 10,
        md5_sum: "00000000000000000000000000000000".to_string(),
        description: None,
        is_primary: None,
    }
}

#[test]
fn import_new_simulation_creates_all_related_rows() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-new";
    let user_id = seed_user_with_orcid(&mut c, "new", orcid);

    let sim = ExportSimulation {
        pdb: Some(PdbEntry {
            pdb_id: "1ABC".into(),
            title: "Test structure".into(),
            classification: "TRANSFERASE".into(),
        }),
        uniprots: vec![UniprotEntry {
            uniprot_id: "P00000-importtest".into(),
            name: "Test protein".into(),
            sequence: "MLDD".into(),
        }],
        external_links: vec![metadata::ExternalLink {
            url: "https://example.org/import-test".into(),
            label: Some("Example".into()),
        }],
        contributors: vec![metadata::Contributor {
            name: "Ada Lovelace".into(),
            orcid: Some("0000-0002-contributor-new".into()),
            email: None,
            institution: None,
        }],
        original_files: vec![md_file("orig.pdb", "Structure")],
        processed_files: vec![md_file("proc1.nc", "Trajectory")],
        replicates: vec!["traj1.xtc".into()],
        ligands: vec![ResolvedLigand {
            name: "TestLigand".into(),
            smiles: "CC".into(),
            inchi: Some("InChI=1S/C2H6/c1-2/h1-2H3".into()),
            inchikey: Some("OTMSDBZUPAUEDD-UHFFFAOYSA-N".into()),
            declared_identity: Some("smiles".into()),
            identity_software: Some("rdkit".into()),
        }],
        solutes: vec![metadata::Solute {
            name: "Na+".into(),
            concentration_mol_liter: 0.15,
        }],
        papers: vec![metadata::Paper {
            title: "A paper with no DOI".into(),
            authors: "Someone".into(),
            journal: "Journal of Testing".into(),
            volume: 1,
            number: None,
            year: 2024,
            pages: None,
            doi: None,
        }],
        collections: vec!["ATLAS-importtest".into(), "GPCR-MD-importtest".into()],
        ..base_sim("new", orcid)
    };

    let sim_id = import::import_simulation(&mut c, &sim, &ImportOpts::default())
        .expect("import should succeed");

    let row = ops::get_simulation(&mut c, sim_id).expect("simulation row exists");
    assert_eq!(row.created_by_id, Some(user_id));
    assert!(
        row.is_placeholder,
        "a freshly imported sim is a placeholder"
    );
    assert!(!row.is_public, "a freshly imported sim starts private");
    assert!(row.pdb_id.is_some(), "pdb should be linked");

    assert!(
        ops::find_uploaded_file_id(&mut c, sim_id, "orig.pdb")
            .unwrap()
            .is_some()
    );
    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc1.nc")
            .unwrap()
            .is_some()
    );
    assert!(
        ops::find_replicate_id(&mut c, sim_id, "traj1.xtc")
            .unwrap()
            .is_some()
    );
    assert!(
        ops::find_contribution_id(
            &mut c,
            sim_id,
            ops::ContributionKey::Orcid("0000-0002-contributor-new")
        )
        .unwrap()
        .is_some()
    );
    assert!(
        ops::find_ligand_id(&mut c, sim_id, "TestLigand")
            .unwrap()
            .is_some()
    );
    assert!(
        ops::find_solute_id(&mut c, sim_id, "Na+")
            .unwrap()
            .is_some()
    );
    assert!(
        ops::find_external_link_id(&mut c, sim_id, "https://example.org/import-test")
            .unwrap()
            .is_some()
    );

    let uniprot_pk = ops::find_uniprot_id_by_accession(&mut c, "P00000-importtest")
        .unwrap()
        .expect("uniprot row created");
    assert!(
        ops::find_simulation_uniprot_id(&mut c, sim_id, uniprot_pk)
            .unwrap()
            .is_some()
    );

    for name in ["ATLAS-importtest", "GPCR-MD-importtest"] {
        let (_, found) =
            ops::list_collections(&mut c, Some(name.to_string()), true, None, None)
                .unwrap();
        let collection = found
            .into_iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("collection {name} should have been created"));
        assert_eq!(collection.user_id, user_id, "owned by the lead contributor");
        assert!(
            ops::find_simulation_collection_id(&mut c, sim_id, collection.id)
                .unwrap()
                .is_some(),
            "collection {name} should be linked to the simulation"
        );
    }
}

#[test]
fn import_same_alias_twice_is_idempotent_not_duplicated() {
    // The pattern every upsert_* helper in import.rs relies on: a second
    // import of the same payload must resolve to the same simulation (via
    // its alias) and refresh each related row rather than inserting a
    // second one -- otherwise every re-run of a landing directory would
    // duplicate its uploaded files, contributors, ligands, etc.
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-idempotent";
    seed_user_with_orcid(&mut c, "idempotent", orcid);

    let make_sim = || ExportSimulation {
        original_files: vec![md_file("orig.pdb", "Structure")],
        contributors: vec![metadata::Contributor {
            name: "Grace Hopper".into(),
            orcid: Some("0000-0002-contributor-idempotent".into()),
            email: None,
            institution: None,
        }],
        ligands: vec![ResolvedLigand {
            name: "IdempotentLigand".into(),
            smiles: "CC".into(),
            inchi: Some("InChI=1S/C2H6/c1-2/h1-2H3".into()),
            inchikey: Some("OTMSDBZUPAUEDD-UHFFFAOYSA-N".into()),
            declared_identity: Some("smiles".into()),
            identity_software: Some("rdkit".into()),
        }],
        collections: vec!["ATLAS-idempotent".into()],
        ..base_sim("idempotent", orcid)
    };

    let first_id =
        import::import_simulation(&mut c, &make_sim(), &ImportOpts::default())
            .expect("first import should succeed");
    let second_id =
        import::import_simulation(&mut c, &make_sim(), &ImportOpts::default())
            .expect("second import should succeed");

    assert_eq!(
        first_id, second_id,
        "same alias should resolve to the same row"
    );

    let uploaded_file_id =
        ops::find_uploaded_file_id(&mut c, first_id, "orig.pdb").unwrap();
    assert!(uploaded_file_id.is_some());

    let contribution_id = ops::find_contribution_id(
        &mut c,
        first_id,
        ops::ContributionKey::Orcid("0000-0002-contributor-idempotent"),
    )
    .unwrap();
    assert!(contribution_id.is_some());

    let ligand_id = ops::find_ligand_id(&mut c, first_id, "IdempotentLigand").unwrap();
    assert!(ligand_id.is_some());

    // Same (user_id, name): the second import must resolve to the same
    // md_collection row, not error on the unique constraint or duplicate the
    // md_simulation_collection link.
    let (count, found) = ops::list_collections(
        &mut c,
        Some("ATLAS-idempotent".to_string()),
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(count, 1, "exactly one Collection row, not two");
    let collection_id = found[0].id;
    assert!(
        ops::find_simulation_collection_id(&mut c, first_id, collection_id)
            .unwrap()
            .is_some()
    );
}

#[test]
fn reprocess_replace_original_files_removes_old_uploaded_and_processed_files() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-replace";
    seed_user_with_orcid(&mut c, "replace", orcid);

    let first_sim = ExportSimulation {
        original_files: vec![md_file("orig.pdb", "Structure")],
        processed_files: vec![md_file("proc1.nc", "Trajectory")],
        ..base_sim("replace", orcid)
    };
    let sim_id = import::import_simulation(&mut c, &first_sim, &ImportOpts::default())
        .expect("first import should succeed");

    let second_sim = ExportSimulation {
        original_files: vec![md_file("orig2.pdb", "Structure")],
        processed_files: vec![md_file("proc2.nc", "Trajectory")],
        ..base_sim("replace", orcid)
    };
    let opts = ImportOpts {
        reprocess_simulation_id: Some(sim_id as u64),
        replace_original_files: true,
        ticket_id: None,
    };
    let reprocessed_id = import::import_simulation(&mut c, &second_sim, &opts)
        .expect("reprocess should succeed");
    assert_eq!(sim_id, reprocessed_id);

    assert!(
        ops::find_uploaded_file_id(&mut c, sim_id, "orig.pdb")
            .unwrap()
            .is_none(),
        "old uploaded file should be removed with --replace-original-files"
    );
    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc1.nc")
            .unwrap()
            .is_none(),
        "old processed file should be removed on every reprocess"
    );
    assert!(
        ops::find_uploaded_file_id(&mut c, sim_id, "orig2.pdb")
            .unwrap()
            .is_some(),
        "new uploaded file should be present"
    );
    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc2.nc")
            .unwrap()
            .is_some(),
        "new processed file should be present"
    );
}

#[test]
fn reprocess_without_replace_original_files_keeps_uploaded_files() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-keep";
    seed_user_with_orcid(&mut c, "keep", orcid);

    let first_sim = ExportSimulation {
        original_files: vec![md_file("orig.pdb", "Structure")],
        processed_files: vec![md_file("proc1.nc", "Trajectory")],
        ..base_sim("keep", orcid)
    };
    let sim_id = import::import_simulation(&mut c, &first_sim, &ImportOpts::default())
        .expect("first import should succeed");

    // A reprocess payload that lists no original files -- the normal shape
    // when only the processed outputs changed.
    let second_sim = ExportSimulation {
        processed_files: vec![md_file("proc2.nc", "Trajectory")],
        ..base_sim("keep", orcid)
    };
    let opts = ImportOpts {
        reprocess_simulation_id: Some(sim_id as u64),
        replace_original_files: false,
        ticket_id: None,
    };
    import::import_simulation(&mut c, &second_sim, &opts)
        .expect("reprocess should succeed");

    assert!(
        ops::find_uploaded_file_id(&mut c, sim_id, "orig.pdb")
            .unwrap()
            .is_some(),
        "uploaded files survive a reprocess without --replace-original-files"
    );
    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc1.nc")
            .unwrap()
            .is_none(),
        "processed files are always replaced on reprocess"
    );
    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc2.nc")
            .unwrap()
            .is_some()
    );
}

#[test]
fn import_with_unknown_explicit_simulation_id_is_an_error() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-unknown-id";
    seed_user_with_orcid(&mut c, "unknown-id", orcid);

    let sim = ExportSimulation {
        simulation_id: Some(999_999_999),
        ..base_sim("unknown-id", orcid)
    };

    let err = import::import_simulation(&mut c, &sim, &ImportOpts::default())
        .expect_err(
            "a nonexistent explicit simulation_id must not silently create a row",
        );
    assert!(err.to_string().contains("Invalid simulation ID"));
}
