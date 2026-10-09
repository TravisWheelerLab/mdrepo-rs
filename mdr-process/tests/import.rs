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
    ExportSimulation, ImportChain, MdFile, PdbEntry, PolymerHit, PolymerLookup,
    ResolvedLigand, UniprotEntry,
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
        creators: vec![],
        original_files: vec![],
        processed_files: vec![],
        replicates: vec![],
        ligands: vec![],
        chains: vec![],
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
            ..Default::default()
        }),
        uniprots: vec![UniprotEntry {
            uniprot_id: "P00000-importtest".into(),
            name: "Test protein".into(),
            sequence: "MLDD".into(),
            ..Default::default()
        }],
        external_links: vec![metadata::ExternalLink {
            url: "https://example.org/import-test".into(),
            label: Some("Example".into()),
        }],
        creators: vec![metadata::Creator {
            name: "Ada Lovelace".into(),
            orcid: Some("0000-0002-contributor-new".into()),
            email: None,
            institution: None,
        }],
        original_files: vec![md_file("orig.pdb", "Structure")],
        processed_files: vec![md_file("proc1.nc", "Processed trajectory")],
        replicates: vec!["traj1.xtc".into()],
        ligands: vec![ResolvedLigand {
            name: "TestLigand".into(),
            smiles: Some("CC".into()),
            chain_order: None,
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
    assert_eq!(row.contributor_id, Some(user_id));
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

    // The md_creator half of the dual-write. Until this existed the tests
    // only checked md_contribution, so the import could have stopped
    // populating the new tables entirely and nothing would have failed --
    // which matters, because the dual-write is the only thing keeping them
    // current for new imports between the backfill and the drop.
    let creators =
        ops::list_creators_for_simulation(&mut c, sim_id).expect("creators load");
    assert_eq!(creators.len(), 1, "the creator should be linked");
    assert_eq!(creators[0].name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        creators[0].orcid.as_deref(),
        Some("0000-0002-contributor-new")
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
    // duplicate its uploaded files, creators, ligands, etc.
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-idempotent";
    seed_user_with_orcid(&mut c, "idempotent", orcid);

    let make_sim = || ExportSimulation {
        original_files: vec![md_file("orig.pdb", "Structure")],
        creators: vec![metadata::Creator {
            name: "Grace Hopper".into(),
            orcid: Some("0000-0002-contributor-idempotent".into()),
            email: None,
            institution: None,
        }],
        ligands: vec![ResolvedLigand {
            name: "IdempotentLigand".into(),
            smiles: Some("CC".into()),
            chain_order: None,
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
    // Still a placeholder, so this is a retry of the same landing, which must
    // stay allowed without --reprocess-simulation-id.
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

    // Re-importing must not duplicate the creator or its link. The creator
    // row is deduped by identity and the link by its composite primary key,
    // so a second import of the same payload is a no-op on both.
    let creators =
        ops::list_creators_for_simulation(&mut c, first_id).expect("creators load");
    assert_eq!(
        creators.len(),
        1,
        "re-import duplicated the simulation's creators: {creators:?}"
    );

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
        processed_files: vec![md_file("proc1.nc", "Processed trajectory")],
        ..base_sim("replace", orcid)
    };
    let sim_id = import::import_simulation(&mut c, &first_sim, &ImportOpts::default())
        .expect("first import should succeed");

    let second_sim = ExportSimulation {
        original_files: vec![md_file("orig2.pdb", "Structure")],
        processed_files: vec![md_file("proc2.nc", "Processed trajectory")],
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
        processed_files: vec![md_file("proc1.nc", "Processed trajectory")],
        ..base_sim("keep", orcid)
    };
    let sim_id = import::import_simulation(&mut c, &first_sim, &ImportOpts::default())
        .expect("first import should succeed");

    // A reprocess payload that lists no original files -- the normal shape
    // when only the processed outputs changed.
    let second_sim = ExportSimulation {
        processed_files: vec![md_file("proc2.nc", "Processed trajectory")],
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

#[test]
fn reimport_without_reprocess_is_refused() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0001-import-refuse";
    seed_user_with_orcid(&mut c, "refuse", orcid);

    let first_sim = ExportSimulation {
        original_files: vec![md_file("orig.pdb", "Structure")],
        processed_files: vec![md_file("proc1.nc", "Processed trajectory")],
        ..base_sim("refuse", orcid)
    };
    let sim_id = import::import_simulation(&mut c, &first_sim, &ImportOpts::default())
        .expect("first import should succeed");
    // What a verified push does; a placeholder would be let through as a retry.
    ops::update_simulation(
        &mut c,
        sim_id,
        SimulationUpdate {
            is_placeholder: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    // Same alias and contributor, renamed files: the shape of a submitter's
    // supersede, which used to add proc2.nc beside proc1.nc (MDR-74).
    let second_sim = ExportSimulation {
        original_files: vec![md_file("orig2.pdb", "Structure")],
        processed_files: vec![md_file("proc2.nc", "Processed trajectory")],
        ..base_sim("refuse", orcid)
    };
    let err = import::import_simulation(&mut c, &second_sim, &ImportOpts::default())
        .expect_err("a re-import without --reprocess-simulation-id must be refused");
    let msg = err.to_string();
    assert!(msg.contains(&format!("MDR{sim_id:08}")), "{msg}");
    assert!(msg.contains("alias"), "{msg}");

    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc2.nc")
            .unwrap()
            .is_none(),
        "the refused import must not add rows"
    );
    assert!(
        ops::find_processed_file_id(&mut c, sim_id, "proc1.nc")
            .unwrap()
            .is_some(),
        "the refused import must not remove rows"
    );
}

// --------------------------------------------------
/// The two chains of a DDD-style complex: the protein the splitter found, and a
/// peptide ligand declared by sequence and matched to a blob residue.
fn chains_and_peptide() -> (Vec<ImportChain>, Vec<ResolvedLigand>) {
    let chain = |order: u32, source: &str, seq: &str, residues: &[&str]| ImportChain {
        chain_order: order,
        chain_label: "A".into(),
        source: source.into(),
        polymer_type: "protein".into(),
        sequence: seq.into(),
        residues: residues.iter().map(|r| r.to_string()).collect(),
        first_residue: (source == "structure").then_some(1),
        last_residue: (source == "structure").then_some(residues.len() as i32),
        n_terminal_cap: None,
        c_terminal_cap: None,
        breaks: vec![],
        reference: None,
    };
    let chains = vec![
        chain(1, "structure", "NLYQ", &["ASN", "LEU", "TYR", "GLN"]),
        chain(2, "declared", "VAFRS", &["VAL", "ALA", "PHE", "ARG", "SER"]),
    ];
    let ligands = vec![
        ResolvedLigand {
            name: "VAFRS peptide".into(),
            smiles: None,
            chain_order: Some(2),
            inchi: None,
            inchikey: None,
            declared_identity: Some("sequence".into()),
            identity_software: None,
        },
        ResolvedLigand {
            name: "ethanol".into(),
            smiles: Some("CCO".into()),
            chain_order: None,
            inchi: None,
            inchikey: None,
            declared_identity: Some("smiles".into()),
            identity_software: None,
        },
    ];
    (chains, ligands)
}

#[test]
fn import_writes_chains_polymers_and_a_peptide_ligand() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0002-0000-0094";
    seed_user_with_orcid(&mut c, "mdr94", orcid);
    let (chains, ligands) = chains_and_peptide();
    let sim = ExportSimulation {
        chains,
        ligands,
        ..base_sim("mdr94", orcid)
    };

    let sim_id =
        import::import_simulation(&mut c, &sim, &ImportOpts::default()).unwrap();

    let got = ops::list_chains_for_simulation(&mut c, sim_id).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(
        (
            got[0].chain_order,
            got[0].source.as_str(),
            got[0].first_residue
        ),
        (1, "structure", Some(1))
    );
    assert_eq!(
        (got[1].chain_order, got[1].source.as_str()),
        (2, "declared")
    );
    let peptide = ops::get_polymer(&mut c, got[1].polymer_id).unwrap();
    assert_eq!(peptide.sequence, "VAFRS");
    assert_eq!(peptide.num_residues, 5);
    assert_eq!(
        peptide.residues_hash,
        import::residues_hash(&peptide.residues)
    );

    let (_, ligs) =
        ops::list_ligands(&mut c, None, Some(sim_id), true, None, None).unwrap();
    let by_name = |n: &str| ligs.iter().find(|l| l.name == n).unwrap();
    assert_eq!(by_name("VAFRS peptide").chain_id, Some(got[1].id));
    assert_eq!(by_name("VAFRS peptide").smiles, None);
    assert_eq!(by_name("ethanol").smiles.as_deref(), Some("CCO"));
    assert_eq!(by_name("ethanol").chain_id, None);
}

#[test]
fn reimporting_replaces_chains_and_shares_polymers() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0002-0000-0095";
    seed_user_with_orcid(&mut c, "mdr94b", orcid);
    let (chains, ligands) = chains_and_peptide();
    let sim = ExportSimulation {
        chains,
        ligands,
        ..base_sim("mdr94b", orcid)
    };

    let first =
        import::import_simulation(&mut c, &sim, &ImportOpts::default()).unwrap();
    let before = ops::list_chains_for_simulation(&mut c, first).unwrap();
    let opts = ImportOpts {
        reprocess_simulation_id: Some(first as u64),
        ..Default::default()
    };
    let again = import::import_simulation(&mut c, &sim, &opts).unwrap();
    assert_eq!(again, first);

    let after = ops::list_chains_for_simulation(&mut c, first).unwrap();
    assert_eq!(after.len(), 2, "replaced, not added to");
    assert_ne!(after[1].id, before[1].id);
    assert_eq!(
        after[1].polymer_id, before[1].polymer_id,
        "one polymer row per sequence"
    );
    let (_, ligs) =
        ops::list_ligands(&mut c, None, Some(first), true, None, None).unwrap();
    assert_eq!(ligs.len(), 2);
    let peptide = ligs.iter().find(|l| l.chain_id.is_some()).unwrap();
    assert_eq!(peptide.chain_id, Some(after[1].id));
}

/// A protein chain of 25 residues with the given lookup
fn looked_up_chain(lookup: Option<PolymerLookup>) -> ImportChain {
    let sequence = "ACDEFGHIKLMNPQRSTVWYACDEF";
    ImportChain {
        chain_order: 1,
        chain_label: "A".into(),
        source: "structure".into(),
        polymer_type: "protein".into(),
        sequence: sequence.into(),
        residues: sequence.chars().map(|l| format!("Z{l}")).collect(),
        first_residue: Some(1),
        last_residue: Some(25),
        n_terminal_cap: None,
        c_terminal_cap: None,
        breaks: vec![],
        reference: lookup,
    }
}

fn hit(method: &str, accession: &str) -> PolymerLookup {
    PolymerLookup {
        match_method: method.into(),
        hit: Some(PolymerHit {
            uniprot: UniprotEntry {
                uniprot_id: accession.into(),
                name: format!("{accession} protein"),
                sequence: "M".repeat(40),
                response: Some(serde_json::json!({"primaryAccession": accession})),
                entry_version: Some(7),
                fetched_at: Some(chrono::Utc::now()),
            },
            query_start: 1,
            query_end: 25,
            reference_start: 11,
            reference_end: 35,
            identity: 96.0,
        }),
    }
}

/// The first import's lookup sets the polymer's reference, with the UniProt
/// entry's raw response; a second simulation with the same residues and a
/// different answer leaves it (set once), and a hit is not linked to the
/// simulation (md_simulation_uniprot keeps the TOML's accessions)
#[test]
fn polymer_reference_is_written_once() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0002-0000-0287";
    seed_user_with_orcid(&mut c, "ref1", orcid);

    let first = ExportSimulation {
        chains: vec![looked_up_chain(Some(hit("pdb", "Q00001-importref")))],
        ..base_sim("ref1", orcid)
    };
    let first_id =
        import::import_simulation(&mut c, &first, &ImportOpts::default()).unwrap();
    let chains = ops::list_chains_for_simulation(&mut c, first_id).unwrap();
    let polymer = ops::get_polymer(&mut c, chains[0].polymer_id).unwrap();
    let q1 = ops::find_uniprot_id_by_accession(&mut c, "Q00001-importref")
        .unwrap()
        .unwrap();
    assert_eq!(polymer.match_method.as_deref(), Some("pdb"));
    assert_eq!(polymer.uniprot_id, Some(q1));
    assert_eq!(
        (polymer.query_start, polymer.query_end),
        (Some(1), Some(25))
    );
    assert_eq!(
        (polymer.reference_start, polymer.reference_end),
        (Some(11), Some(35))
    );
    assert_eq!(polymer.identity, Some(96.0));
    let uniprot = ops::get_uniprot(&mut c, q1).unwrap();
    assert_eq!(uniprot.entry_version, Some(7));
    assert_eq!(
        uniprot.response,
        Some(serde_json::json!({"primaryAccession": "Q00001-importref"}))
    );
    assert!(uniprot.fetched_at.is_some());
    assert_eq!(
        ops::find_simulation_uniprot_id(&mut c, first_id, q1).unwrap(),
        None
    );

    let second = ExportSimulation {
        chains: vec![looked_up_chain(Some(hit("aligned", "Q00002-importref")))],
        ..base_sim("ref2", orcid)
    };
    let second_id =
        import::import_simulation(&mut c, &second, &ImportOpts::default()).unwrap();
    assert_ne!(second_id, first_id);
    let chains = ops::list_chains_for_simulation(&mut c, second_id).unwrap();
    assert_eq!(chains[0].polymer_id, polymer.id, "one polymer row");
    let again = ops::get_polymer(&mut c, polymer.id).unwrap();
    assert_eq!(again.uniprot_id, Some(q1));
    assert_eq!(again.match_method.as_deref(), Some("pdb"));
    assert_eq!(
        ops::find_uniprot_id_by_accession(&mut c, "Q00002-importref").unwrap(),
        None,
        "a hit that is not written is not stored"
    );
}

/// No lookup (None) leaves the polymer untried for a later run; `none` is
/// recorded and sticks
#[test]
fn polymer_lookup_none_and_untried() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0002-0000-0288";
    seed_user_with_orcid(&mut c, "ref3", orcid);

    let untried = ExportSimulation {
        chains: vec![looked_up_chain(None)],
        ..base_sim("ref3", orcid)
    };
    let sim_id =
        import::import_simulation(&mut c, &untried, &ImportOpts::default()).unwrap();
    let chains = ops::list_chains_for_simulation(&mut c, sim_id).unwrap();
    let polymer_id = chains[0].polymer_id;
    assert_eq!(
        ops::get_polymer(&mut c, polymer_id).unwrap().match_method,
        None
    );

    let none = ExportSimulation {
        chains: vec![looked_up_chain(Some(PolymerLookup {
            match_method: "none".into(),
            hit: None,
        }))],
        ..base_sim("ref4", orcid)
    };
    import::import_simulation(&mut c, &none, &ImportOpts::default()).unwrap();
    let polymer = ops::get_polymer(&mut c, polymer_id).unwrap();
    assert_eq!(
        (polymer.match_method.as_deref(), polymer.uniprot_id),
        (Some("none"), None)
    );
}

/// A later simulation whose own lookup disagrees with the polymer's stored
/// reference gets a warning; one that agrees, or a polymer never looked up,
/// gets none
#[test]
fn reference_disagreement_warns() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0002-0000-0291";
    seed_user_with_orcid(&mut c, "ref5", orcid);

    let first = ExportSimulation {
        chains: vec![looked_up_chain(Some(hit("pdb", "Q00011-importref")))],
        ..base_sim("ref5", orcid)
    };
    assert!(
        import::reference_disagreements(&mut c, &first)
            .unwrap()
            .is_empty(),
        "nothing stored yet"
    );
    import::import_simulation(&mut c, &first, &ImportOpts::default()).unwrap();

    let agrees = ExportSimulation {
        chains: vec![looked_up_chain(Some(hit("aligned", "Q00011-importref")))],
        ..base_sim("ref6", orcid)
    };
    assert!(
        import::reference_disagreements(&mut c, &agrees)
            .unwrap()
            .is_empty()
    );

    let differs = ExportSimulation {
        chains: vec![looked_up_chain(Some(hit("pdb", "Q00012-importref")))],
        ..base_sim("ref7", orcid)
    };
    let got = import::reference_disagreements(&mut c, &differs).unwrap();
    assert_eq!(got.len(), 1);
    assert!(got[0].contains("Q00012-importref"), "{}", got[0]);
    assert!(got[0].contains("Q00011-importref"), "{}", got[0]);
    assert!(got[0].starts_with("Chain 1 (A)"), "{}", got[0]);
}

/// The chain backfill writes a simulation's chains, polymers and references
/// once; a simulation that already has chains is left alone
#[test]
fn backfill_writes_chains_once() {
    let mut c = conn_or_skip!();
    let orcid = "0000-0002-0000-0292";
    seed_user_with_orcid(&mut c, "bf1", orcid);
    let sim_id = import::import_simulation(
        &mut c,
        &base_sim("bf1", orcid),
        &ImportOpts::default(),
    )
    .unwrap();
    assert!(
        ops::list_chains_for_simulation(&mut c, sim_id)
            .unwrap()
            .is_empty()
    );

    let chains = vec![looked_up_chain(Some(hit("pdb", "Q00021-importref")))];
    assert!(import::backfill_chains(&mut c, sim_id, &chains).unwrap());
    let got = ops::list_chains_for_simulation(&mut c, sim_id).unwrap();
    assert_eq!(got.len(), 1);
    let polymer = ops::get_polymer(&mut c, got[0].polymer_id).unwrap();
    assert_eq!(polymer.match_method.as_deref(), Some("pdb"));

    // Again: left alone, nothing added
    let other = vec![looked_up_chain(Some(hit("aligned", "Q00022-importref")))];
    assert!(!import::backfill_chains(&mut c, sim_id, &other).unwrap());
    assert_eq!(
        ops::list_chains_for_simulation(&mut c, sim_id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ops::find_uniprot_id_by_accession(&mut c, "Q00022-importref").unwrap(),
        None
    );
}
