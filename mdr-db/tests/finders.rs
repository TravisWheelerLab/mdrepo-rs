//! Integration tests for the import natural-key finders, the reprocess
//! delete-cascade helpers, and the public-visibility selector in `mdr_db::ops`.
//!
//! These need a Postgres loaded with the mdrepo schema. The easiest way to run
//! them is `mdr-db/tests/with-testdb.sh`, which spins up an ephemeral container,
//! loads `tests/fixtures/schema.sql`, and points `TEST_DATABASE_URL` at it.
//!
//! Each test opens its own connection in a `begin_test_transaction()` — an
//! transaction that is never committed — so every row a test inserts (and every
//! delete it exercises) is rolled back when the connection drops. That keeps the
//! destructive delete helpers safe to test and needs no cleanup between tests.
//!
//! When `TEST_DATABASE_URL` is unset the tests skip (and pass), so a plain
//! `cargo test` stays green for anyone without Docker; CI sets the variable.

use chrono::Utc;
use diesel::prelude::*;
use mdr_db::models::*;
use mdr_db::ops;

/// A connection whose work always rolls back, or `None` when the test DB isn't
/// configured (so the caller can skip).
fn test_conn() -> Option<PgConnection> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let mut conn = PgConnection::establish(&url).expect("connect to TEST_DATABASE_URL");
    conn.begin_test_transaction()
        .expect("begin rolled-back test transaction");
    Some(conn)
}

/// Grab a rolled-back connection, or skip the test if the DB isn't configured.
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

// ── seed helpers ──────────────────────────────────────────────────────────────
// Distinct-per-test data avoids contention on unique indexes when cargo runs
// tests in parallel (each in its own uncommitted transaction).

fn seed_user(c: &mut PgConnection, key: &str) -> i64 {
    ops::insert_user(
        c,
        NewUser {
            password: "!".into(),
            is_superuser: false,
            username: format!("user-{key}"),
            is_staff: false,
            date_joined: Utc::now(),
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
    .id
}

fn seed_orcid(c: &mut PgConnection, user_id: i64, provider: &str, orcid: &str) {
    ops::insert_social_account(
        c,
        NewSocialAccount {
            provider: provider.into(),
            uid: orcid.into(),
            last_login: Utc::now(),
            date_joined: Utc::now(),
            extra_data: serde_json::json!({}),
            user_id,
        },
    )
    .expect("insert social account");
}

fn seed_sim(c: &mut PgConnection) -> i64 {
    // The historical default: a placeholder, unapproved simulation.
    seed_sim_with_flags(c, false, false, true, false)
}

/// Seed a simulation with the four flags that decide public visibility set
/// explicitly, so a test can say exactly which corner it is exercising.
fn seed_sim_with_flags(
    c: &mut PgConnection,
    public: bool,
    deprecated: bool,
    placeholder: bool,
    embargoed: bool,
) -> i64 {
    ops::insert_simulation(
        c,
        NewSimulation {
            description: None,
            short_description: "test simulation".into(),
            run_commands: None,
            water_type: None,
            water_density: None,
            duration: None,
            sampling_frequency: None,
            sampling_frequency_ps: None,
            integration_timestep_fs: None,
            creation_date: Utc::now(),
            software_id: None,
            created_by_id: None,
            unique_file_hash_string: None,
            rmsd_values: None,
            rmsf_values: None,
            forcefield: None,
            forcefield_comments: None,
            temperature: None,
            is_placeholder: placeholder,
            is_deprecated: deprecated,
            is_public: public,
            protonation_method: None,
            fasta_sequence: None,
            alias: None,
            pdb_id: None,
            is_embargoed: embargoed,
            is_coarse_grained: false,
            num_replicates: None,
            irods_ticket: None,
            superseding_simulation_id: None,
            md_repo_ticket_id: None,
        },
    )
    .expect("insert simulation")
    .id
}

fn seed_processed_file(c: &mut PgConnection, sim_id: i64, name: &str) -> i64 {
    ops::insert_processed_file(
        c,
        NewProcessedFile {
            file_type: "psf".into(),
            local_file_path: format!("/data/{name}"),
            filename: name.into(),
            simulation_id: sim_id,
            file_size_bytes: None,
            description: None,
            md5_hash: None,
        },
    )
    .expect("insert processed file")
    .id
}

fn seed_uploaded_file(c: &mut PgConnection, sim_id: i64, name: &str) -> i64 {
    ops::insert_uploaded_file(
        c,
        NewUploadedFile {
            filename: name.into(),
            file_type: "psf".into(),
            simulation_id: sim_id,
            description: None,
            local_file_path: format!("/data/{name}"),
            file_size_bytes: None,
            md5_hash: None,
            is_primary: false,
        },
    )
    .expect("insert uploaded file")
    .id
}

fn seed_download_instance(c: &mut PgConnection, sim_id: i64) -> i64 {
    ops::insert_download_instance(
        c,
        NewDownloadInstance {
            created_on: Utc::now(),
            used: false,
            simulation_id: sim_id,
            user_id: None,
        },
    )
    .expect("insert download instance")
    .id
}

fn seed_replicate(c: &mut PgConnection, sim_id: i64, name: &str) -> i64 {
    ops::insert_replicate(
        c,
        NewReplicate {
            trajectory_file_name: name.into(),
            simulation_id: sim_id,
        },
    )
    .expect("insert replicate")
    .id
}

/// Link a simulation to a fresh accession. `acc` must be distinct per test:
/// `md_uniprot` is unique on the accession and tests run in parallel.
fn seed_simulation_uniprot(c: &mut PgConnection, sim_id: i64, acc: &str) -> (i64, i64) {
    let uniprot = ops::insert_uniprot(
        c,
        NewUniprot {
            uniprot_id: acc.into(),
            name: format!("{acc}_HUMAN"),
            amino_length: 42,
            sequence: "MTEST".into(),
        },
    )
    .expect("insert uniprot")
    .id;

    let link = ops::insert_simulation_uniprot(
        c,
        NewSimulationUniprot {
            simulation_id: sim_id,
            uniprot_id: uniprot,
        },
    )
    .expect("insert simulation uniprot")
    .id;

    (uniprot, link)
}

/// Set the alias / creator / file-hash keys the alias & hash finders look up.
fn set_sim_keys(
    c: &mut PgConnection,
    sim_id: i64,
    alias: Option<&str>,
    created_by: Option<i64>,
    hash: Option<&str>,
) {
    ops::update_simulation(
        c,
        sim_id,
        SimulationUpdate {
            alias: Some(alias.map(String::from)),
            created_by_id: Some(created_by),
            unique_file_hash_string: Some(hash.map(String::from)),
            ..Default::default()
        },
    )
    .expect("set simulation keys");
}

// ── finders ───────────────────────────────────────────────────────────────────

#[test]
fn find_user_id_by_orcid_matches_only_orcid_provider() {
    let mut c = conn_or_skip!();
    let uid = seed_user(&mut c, "orcidtest");
    seed_orcid(&mut c, uid, "orcid", "0000-0001-orcidtest");
    // A same-uid account under a different provider must NOT match.
    let other = seed_user(&mut c, "githubtest");
    seed_orcid(&mut c, other, "github", "0000-0009-nomatch");

    assert_eq!(
        ops::find_user_id_by_orcid(&mut c, "0000-0001-orcidtest").unwrap(),
        Some(uid)
    );
    assert_eq!(
        ops::find_user_id_by_orcid(&mut c, "0000-0009-nomatch").unwrap(),
        None
    );
    assert_eq!(
        ops::find_user_id_by_orcid(&mut c, "does-not-exist").unwrap(),
        None
    );
}

#[test]
fn find_user_id_by_username_matches_exact_username() {
    let mut c = conn_or_skip!();
    let uid = seed_user(&mut c, "usernametest");

    assert_eq!(
        ops::find_user_id_by_username(&mut c, "user-usernametest").unwrap(),
        Some(uid)
    );
    assert_eq!(
        ops::find_user_id_by_username(&mut c, "does-not-exist").unwrap(),
        None
    );
}

#[test]
fn find_software_matches_null_version() {
    let mut c = conn_or_skip!();
    let versionless = ops::insert_software(
        &mut c,
        NewSoftware {
            name: "GROMACS-nulltest".into(),
            version: None,
        },
    )
    .unwrap()
    .id;
    let versioned = ops::insert_software(
        &mut c,
        NewSoftware {
            name: "GROMACS-vtest".into(),
            version: Some("2024".into()),
        },
    )
    .unwrap()
    .id;

    // None matches the NULL-version row (the deliberate improvement over the
    // Python, whose `version = NULL` never matched).
    assert_eq!(
        ops::find_software_id_by_name_version(&mut c, "GROMACS-nulltest", None)
            .unwrap(),
        Some(versionless)
    );
    // A version query does not match the NULL-version row.
    assert_eq!(
        ops::find_software_id_by_name_version(&mut c, "GROMACS-nulltest", Some("2024"))
            .unwrap(),
        None
    );
    // Exact (name, version) match.
    assert_eq!(
        ops::find_software_id_by_name_version(&mut c, "GROMACS-vtest", Some("2024"))
            .unwrap(),
        Some(versioned)
    );
    // Unknown name.
    assert_eq!(
        ops::find_software_id_by_name_version(&mut c, "AMBER-none", None).unwrap(),
        None
    );
}

#[test]
fn find_pub_by_doi_and_metadata() {
    let mut c = conn_or_skip!();
    let p = ops::insert_pub(
        &mut c,
        NewPub {
            title: "A Title (pubtest)".into(),
            authors: "Doe, J.".into(),
            journal: "J. Testing".into(),
            volume: 12,
            number: None,
            year: 2025,
            pages: None,
            doi: Some("10.1000/pubtest".into()),
        },
    )
    .unwrap()
    .id;

    assert_eq!(
        ops::find_pub_id_by_doi(&mut c, "10.1000/pubtest").unwrap(),
        Some(p)
    );
    assert_eq!(
        ops::find_pub_id_by_doi(&mut c, "10.1000/absent").unwrap(),
        None
    );
    assert_eq!(
        ops::find_pub_id_by_metadata(
            &mut c,
            "A Title (pubtest)",
            "Doe, J.",
            "J. Testing",
            12,
            2025
        )
        .unwrap(),
        Some(p)
    );
    // A differing field (year) means no metadata match.
    assert_eq!(
        ops::find_pub_id_by_metadata(
            &mut c,
            "A Title (pubtest)",
            "Doe, J.",
            "J. Testing",
            12,
            2099
        )
        .unwrap(),
        None
    );
}

#[test]
fn find_simulation_pub_link() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let p = ops::insert_pub(
        &mut c,
        NewPub {
            title: "Linked (linktest)".into(),
            authors: "Roe, R.".into(),
            journal: "J. Links".into(),
            volume: 1,
            number: None,
            year: 2025,
            pages: None,
            doi: Some("10.1000/linktest".into()),
        },
    )
    .unwrap()
    .id;
    let link = ops::insert_simulation_pub(
        &mut c,
        NewSimulationPub {
            simulation_id: sim,
            pub_id: p,
        },
    )
    .unwrap()
    .id;

    assert_eq!(
        ops::find_simulation_pub_id(&mut c, sim, p).unwrap(),
        Some(link)
    );
    assert_eq!(
        ops::find_simulation_pub_id(&mut c, sim, p + 1).unwrap(),
        None
    );
}

#[test]
fn find_simulation_by_alias_scoped_to_creator() {
    let mut c = conn_or_skip!();
    let user = seed_user(&mut c, "aliastest");
    let owned = seed_sim(&mut c);
    set_sim_keys(&mut c, owned, Some("owned-alias"), Some(user), None);
    let anon = seed_sim(&mut c);
    set_sim_keys(&mut c, anon, Some("anon-alias"), None, None);

    assert_eq!(
        ops::find_simulation_id_by_alias(&mut c, "owned-alias", Some(user)).unwrap(),
        Some(owned)
    );
    // Same alias, wrong / missing creator -> no match.
    assert_eq!(
        ops::find_simulation_id_by_alias(&mut c, "owned-alias", None).unwrap(),
        None
    );
    assert_eq!(
        ops::find_simulation_id_by_alias(&mut c, "owned-alias", Some(user + 999_999))
            .unwrap(),
        None
    );
    // A None creator matches a NULL-created row.
    assert_eq!(
        ops::find_simulation_id_by_alias(&mut c, "anon-alias", None).unwrap(),
        Some(anon)
    );
    assert_eq!(
        ops::find_simulation_id_by_alias(&mut c, "absent", Some(user)).unwrap(),
        None
    );
}

#[test]
fn find_simulation_by_hash() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    set_sim_keys(&mut c, sim, None, None, Some("hash-abc-123"));

    assert_eq!(
        ops::find_simulation_id_by_hash(&mut c, "hash-abc-123").unwrap(),
        Some(sim)
    );
    assert_eq!(
        ops::find_simulation_id_by_hash(&mut c, "nope").unwrap(),
        None
    );
}

#[test]
fn find_contribution_by_orcid_email_or_name() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other_sim = seed_sim(&mut c);
    let contrib = ops::insert_contribution(
        &mut c,
        NewContribution {
            email: Some("ada@example.org".into()),
            institution: None,
            name: Some("Ada Lovelace".into()),
            orcid: Some("0000-0002-contribtest".into()),
            simulation_id: Some(sim),
            rank: 1,
        },
    )
    .unwrap()
    .id;

    for key in [
        ops::ContributionKey::Orcid("0000-0002-contribtest"),
        ops::ContributionKey::Email("ada@example.org"),
        ops::ContributionKey::Name("Ada Lovelace"),
    ] {
        assert_eq!(
            ops::find_contribution_id(&mut c, sim, key).unwrap(),
            Some(contrib)
        );
    }
    // Contributors are scoped to their simulation.
    assert_eq!(
        ops::find_contribution_id(
            &mut c,
            other_sim,
            ops::ContributionKey::Orcid("0000-0002-contribtest")
        )
        .unwrap(),
        None
    );
    assert_eq!(
        ops::find_contribution_id(&mut c, sim, ops::ContributionKey::Name("Nobody"))
            .unwrap(),
        None
    );
}

#[test]
fn find_sim_scoped_children_by_natural_key() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other = seed_sim(&mut c);

    let pf = seed_processed_file(&mut c, sim, "run.psf");
    let uf = seed_uploaded_file(&mut c, sim, "run.dcd");
    let rep = ops::insert_replicate(
        &mut c,
        NewReplicate {
            trajectory_file_name: "rep1.xtc".into(),
            simulation_id: sim,
        },
    )
    .unwrap()
    .id;
    let lig = ops::insert_ligand(
        &mut c,
        NewLigand {
            name: "ATP".into(),
            smiles: "C1=NC2=C(C(=N1)N)N=CN2".into(),
            inchi: None,
            inchikey: None,
            declared_identity: Some("smiles".into()),
            identity_software: None,
            simulation_id: sim,
        },
    )
    .unwrap()
    .id;
    let sol = ops::insert_solute(
        &mut c,
        NewSolute {
            name: "Na+".into(),
            concentration: 0.15,
            simulation_id: sim,
        },
    )
    .unwrap()
    .id;
    let link = ops::insert_external_link(
        &mut c,
        NewExternalLink {
            url: "https://example.org/sim".into(),
            label: None,
            simulation_id: sim,
        },
    )
    .unwrap()
    .id;

    assert_eq!(
        ops::find_processed_file_id(&mut c, sim, "run.psf").unwrap(),
        Some(pf)
    );
    assert_eq!(
        ops::find_uploaded_file_id(&mut c, sim, "run.dcd").unwrap(),
        Some(uf)
    );
    assert_eq!(
        ops::find_replicate_id(&mut c, sim, "rep1.xtc").unwrap(),
        Some(rep)
    );
    assert_eq!(ops::find_ligand_id(&mut c, sim, "ATP").unwrap(), Some(lig));
    assert_eq!(ops::find_solute_id(&mut c, sim, "Na+").unwrap(), Some(sol));
    assert_eq!(
        ops::find_external_link_id(&mut c, sim, "https://example.org/sim").unwrap(),
        Some(link)
    );

    // Each is scoped to its own simulation…
    assert_eq!(
        ops::find_processed_file_id(&mut c, other, "run.psf").unwrap(),
        None
    );
    assert_eq!(
        ops::find_uploaded_file_id(&mut c, other, "run.dcd").unwrap(),
        None
    );
    assert_eq!(
        ops::find_replicate_id(&mut c, other, "rep1.xtc").unwrap(),
        None
    );
    assert_eq!(ops::find_ligand_id(&mut c, other, "ATP").unwrap(), None);
    assert_eq!(ops::find_solute_id(&mut c, other, "Na+").unwrap(), None);
    assert_eq!(
        ops::find_external_link_id(&mut c, other, "https://example.org/sim").unwrap(),
        None
    );
    // …and an unknown key finds nothing.
    assert_eq!(ops::find_ligand_id(&mut c, sim, "absent").unwrap(), None);
}

#[test]
fn upsert_uniprot_is_idempotent_on_the_accession() {
    // The regression is ticket 2175 (2026-08-09): two landing directories
    // processed in parallel both cited P35557, both found nothing, and both
    // inserted -- the loser died on the unique constraint and failed its whole
    // directory. A second upsert of the same accession must refresh the row
    // rather than raise, and must return the same primary key.
    let mut c = conn_or_skip!();

    let first = ops::upsert_uniprot(
        &mut c,
        NewUniprot {
            uniprot_id: "P35557-upserttest".into(),
            name: "Glucokinase".into(),
            amino_length: 4,
            sequence: "MLDD".into(),
        },
    )
    .unwrap();

    let second = ops::upsert_uniprot(
        &mut c,
        NewUniprot {
            uniprot_id: "P35557-upserttest".into(),
            name: "Glucokinase, refreshed".into(),
            amino_length: 6,
            sequence: "MLDDRA".into(),
        },
    )
    .unwrap();

    // Same row, not a second one...
    assert_eq!(first.id, second.id);
    assert_eq!(
        ops::find_uniprot_id_by_accession(&mut c, "P35557-upserttest").unwrap(),
        Some(first.id)
    );

    // ...and the payload won, matching what the old update branch wrote.
    assert_eq!(second.name, "Glucokinase, refreshed");
    assert_eq!(second.amino_length, 6);
    assert_eq!(second.sequence, "MLDDRA");
}

#[test]
fn upsert_collection_is_idempotent_on_user_and_name() {
    // Unlike upsert_uniprot, a second upsert of the same (user_id, name) must
    // NOT overwrite description -- the fast path returns the existing row
    // untouched, and even the slow path's ON CONFLICT only self-assigns name.
    let mut c = conn_or_skip!();
    let user_id = seed_user(&mut c, "collection-upsert-test");

    let first = ops::upsert_collection(
        &mut c,
        NewCollection {
            user_id,
            name: "ATLAS-upserttest".into(),
            description: Some("first description".into()),
        },
    )
    .unwrap();

    let second = ops::upsert_collection(
        &mut c,
        NewCollection {
            user_id,
            name: "ATLAS-upserttest".into(),
            description: Some("second description -- must be ignored".into()),
        },
    )
    .unwrap();

    // Same row, not a second one...
    assert_eq!(first.id, second.id);

    // ...and description is NOT refreshed, unlike uniprot's fields above.
    assert_eq!(second.description, Some("first description".to_string()));
}

#[test]
fn upsert_collection_scopes_the_same_name_per_user() {
    // The chosen design is per-owner uniqueness, not global-by-name like
    // uniprot_id/pdb_id: two different users' collections named identically
    // must be two different rows.
    let mut c = conn_or_skip!();
    let user_a = seed_user(&mut c, "collection-scope-a");
    let user_b = seed_user(&mut c, "collection-scope-b");

    let a = ops::upsert_collection(
        &mut c,
        NewCollection {
            user_id: user_a,
            name: "GPCR-MD".into(),
            description: None,
        },
    )
    .unwrap();

    let b = ops::upsert_collection(
        &mut c,
        NewCollection {
            user_id: user_b,
            name: "GPCR-MD".into(),
            description: None,
        },
    )
    .unwrap();

    assert_ne!(a.id, b.id);
}

#[test]
fn upsert_software_is_idempotent_on_name_and_version() {
    // The one that had already done damage: md_software had no unique
    // constraint, so the losing thread inserted a duplicate silently rather
    // than failing. AMBER/2022 existed twice on prod. Depends on the constraint
    // added in Django migration 0258, mirrored into tests/fixtures/schema.sql.
    let mut c = conn_or_skip!();

    let first = ops::upsert_software(
        &mut c,
        NewSoftware {
            name: "AMBER-upserttest".into(),
            version: Some("2022".into()),
        },
    )
    .unwrap();

    let second = ops::upsert_software(
        &mut c,
        NewSoftware {
            name: "AMBER-upserttest".into(),
            version: Some("2022".into()),
        },
    )
    .unwrap();

    assert_eq!(first.id, second.id);

    // A different version is a different row, not a conflict.
    let other = ops::upsert_software(
        &mut c,
        NewSoftware {
            name: "AMBER-upserttest".into(),
            version: Some("2023".into()),
        },
    )
    .unwrap();
    assert_ne!(first.id, other.id);
}

#[test]
fn upsert_pdb_is_idempotent_on_the_code() {
    // Same race as upsert_uniprot_is_idempotent_on_the_accession, against
    // md_pdb's UNIQUE (pdb_id). Fixed before it was observed in the wild.
    let mut c = conn_or_skip!();

    let first = ops::upsert_pdb(
        &mut c,
        NewPdb {
            pdb_id: "1v4t-upserttest".into(),
            classification: Some("TRANSFERASE".into()),
            title: Some("Glucokinase".into()),
        },
    )
    .unwrap();

    let second = ops::upsert_pdb(
        &mut c,
        NewPdb {
            pdb_id: "1v4t-upserttest".into(),
            classification: Some("TRANSFERASE, refreshed".into()),
            title: Some("Glucokinase, refreshed".into()),
        },
    )
    .unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(
        ops::find_pdb_id_by_code(&mut c, "1v4t-upserttest").unwrap(),
        Some(first.id)
    );
    assert_eq!(second.title.as_deref(), Some("Glucokinase, refreshed"));
    assert_eq!(
        second.classification.as_deref(),
        Some("TRANSFERASE, refreshed")
    );
}

#[test]
fn find_uniprot_and_pdb_by_their_string_keys() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let uni = ops::insert_uniprot(
        &mut c,
        NewUniprot {
            uniprot_id: "P0DTC2-unitest".into(),
            name: "Spike".into(),
            amino_length: 4,
            sequence: "MFVF".into(),
        },
    )
    .unwrap()
    .id;
    let pdb = ops::insert_pdb(
        &mut c,
        NewPdb {
            pdb_id: "6vxx-test".into(),
            classification: Some("VIRAL PROTEIN".into()),
            title: Some("Spike".into()),
        },
    )
    .unwrap()
    .id;

    assert_eq!(
        ops::find_uniprot_id_by_accession(&mut c, "P0DTC2-unitest").unwrap(),
        Some(uni)
    );
    assert_eq!(
        ops::find_uniprot_id_by_accession(&mut c, "absent").unwrap(),
        None
    );
    assert_eq!(
        ops::find_pdb_id_by_code(&mut c, "6vxx-test").unwrap(),
        Some(pdb)
    );
    assert_eq!(ops::find_pdb_id_by_code(&mut c, "absent").unwrap(), None);

    // The simulation link is separate from the uniprot row itself.
    assert_eq!(
        ops::find_simulation_uniprot_id(&mut c, sim, uni).unwrap(),
        None
    );
    let link = ops::insert_simulation_uniprot(
        &mut c,
        NewSimulationUniprot {
            simulation_id: sim,
            uniprot_id: uni,
        },
    )
    .unwrap()
    .id;
    assert_eq!(
        ops::find_simulation_uniprot_id(&mut c, sim, uni).unwrap(),
        Some(link)
    );
}

// ── reprocess delete-cascade ──────────────────────────────────────────────────

#[test]
fn delete_processed_files_removes_files_and_links_only_for_sim() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other_sim = seed_sim(&mut c);
    let pf = seed_processed_file(&mut c, sim, "target.psf");
    let other_pf = seed_processed_file(&mut c, other_sim, "keep.psf");

    let di = seed_download_instance(&mut c, sim);
    ops::insert_download_processed_file(
        &mut c,
        NewDownloadProcessedFile {
            frontenddownloadinstance_id: di,
            simulationprocessedfile_id: pf,
        },
    )
    .unwrap();

    let n = ops::delete_processed_files_for_simulation(&mut c, sim).unwrap();
    assert_eq!(n, 1, "one processed file deleted");
    assert!(
        ops::get_processed_file(&mut c, pf).is_err(),
        "target file gone"
    );
    assert!(
        ops::get_processed_file(&mut c, other_pf).is_ok(),
        "other sim's file untouched"
    );
    let (links, _) =
        ops::list_download_processed_files(&mut c, Some(di), None, true, None, None)
            .unwrap();
    assert_eq!(links, 0, "download link removed");
}

#[test]
fn delete_uploaded_files_removes_files_and_links_only_for_sim() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other_sim = seed_sim(&mut c);
    let uf = seed_uploaded_file(&mut c, sim, "target.dcd");
    let other_uf = seed_uploaded_file(&mut c, other_sim, "keep.dcd");

    let di = seed_download_instance(&mut c, sim);
    ops::insert_download_uploaded_file(
        &mut c,
        NewDownloadUploadedFile {
            frontenddownloadinstance_id: di,
            simulationuploadedfile_id: uf,
        },
    )
    .unwrap();

    let n = ops::delete_uploaded_files_for_simulation(&mut c, sim).unwrap();
    assert_eq!(n, 1, "one uploaded file deleted");
    assert!(
        ops::get_uploaded_file(&mut c, uf).is_err(),
        "target file gone"
    );
    assert!(
        ops::get_uploaded_file(&mut c, other_uf).is_ok(),
        "other sim's file untouched"
    );
    let (links, _) =
        ops::list_download_uploaded_files(&mut c, Some(di), None, true, None, None)
            .unwrap();
    assert_eq!(links, 0, "download link removed");
}

#[test]
fn delete_replicates_removes_rows_only_for_sim() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other_sim = seed_sim(&mut c);
    let r1 = seed_replicate(&mut c, sim, "old_R1.xtc");
    let r2 = seed_replicate(&mut c, sim, "old_R2.xtc");
    let other_r = seed_replicate(&mut c, other_sim, "keep_R1.xtc");

    let n = ops::delete_replicates_for_simulation(&mut c, sim).unwrap();
    assert_eq!(n, 2, "both replicates deleted");
    assert!(ops::get_replicate(&mut c, r1).is_err(), "first row gone");
    assert!(ops::get_replicate(&mut c, r2).is_err(), "second row gone");
    assert!(
        ops::get_replicate(&mut c, other_r).is_ok(),
        "other sim's replicate untouched"
    );
}

#[test]
fn delete_replicates_on_a_sim_with_none_is_a_noop() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);

    let n = ops::delete_replicates_for_simulation(&mut c, sim).unwrap();
    assert_eq!(n, 0, "nothing to delete, and no error");
}

#[test]
fn delete_uniprots_removes_links_but_keeps_the_accession() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other_sim = seed_sim(&mut c);
    let (acc, link) = seed_simulation_uniprot(&mut c, sim, "T00001");
    let (other_acc, other_link) = seed_simulation_uniprot(&mut c, other_sim, "T00002");

    let n = ops::delete_uniprots_for_simulation(&mut c, sim).unwrap();
    assert_eq!(n, 1, "one link deleted");
    assert!(
        ops::get_simulation_uniprot(&mut c, link).is_err(),
        "link gone"
    );
    assert!(
        ops::get_simulation_uniprot(&mut c, other_link).is_ok(),
        "other sim's link untouched"
    );

    // The accessions are shared across simulations -- clearing a simulation's
    // links must not delete the protein records themselves.
    assert!(
        ops::get_uniprot(&mut c, acc).is_ok(),
        "accession kept, only the link was cleared"
    );
    assert!(
        ops::get_uniprot(&mut c, other_acc).is_ok(),
        "other accession kept"
    );
}

// ── get_visible_simulation_ids ────────────────────────────────────────────────
//
// These pin the predicate that decides what the public static export on
// static.mdrepo.org publishes. A regression here does not fail loudly -- it
// silently ships embargoed or unapproved science to a world-readable tarball --
// so each of the three suppressing flags gets its own case rather than one
// combined "happy path" assertion.

#[test]
fn visible_ids_include_a_fully_released_simulation() {
    let mut c = conn_or_skip!();
    let sim = seed_sim_with_flags(&mut c, true, false, false, false);

    let ids = ops::get_visible_simulation_ids(&mut c).unwrap();
    assert!(ids.contains(&sim), "released simulation is visible");
}

#[test]
fn visible_ids_exclude_unapproved_embargoed_deprecated_and_placeholder() {
    let mut c = conn_or_skip!();

    let released = seed_sim_with_flags(&mut c, true, false, false, false);
    // Each of these differs from "released" in exactly one flag.
    let unapproved = seed_sim_with_flags(&mut c, false, false, false, false);
    let embargoed = seed_sim_with_flags(&mut c, true, false, false, true);
    let deprecated = seed_sim_with_flags(&mut c, true, true, false, false);
    let placeholder = seed_sim_with_flags(&mut c, true, false, true, false);

    let ids = ops::get_visible_simulation_ids(&mut c).unwrap();

    assert!(ids.contains(&released), "control row is visible");
    assert!(!ids.contains(&unapproved), "unapproved is hidden");
    assert!(
        !ids.contains(&embargoed),
        "embargoed is hidden even though is_public is true -- embargo \
         outlives approval, so is_public alone is not the gate"
    );
    assert!(!ids.contains(&deprecated), "deprecated is hidden");
    assert!(!ids.contains(&placeholder), "placeholder is hidden");
}

#[test]
fn visible_ids_are_a_subset_of_all_ids_and_ascending() {
    let mut c = conn_or_skip!();
    seed_sim_with_flags(&mut c, true, false, false, false);
    seed_sim_with_flags(&mut c, false, false, false, false);

    let all = ops::get_all_simulation_ids(&mut c).unwrap();
    let visible = ops::get_visible_simulation_ids(&mut c).unwrap();

    assert!(
        visible.iter().all(|id| all.contains(id)),
        "visible is a subset of all"
    );
    // mdr-export names each output file after the id, so a stable ascending
    // order is what makes a nightly tarball diffable against the previous one.
    let mut sorted = visible.clone();
    sorted.sort();
    assert_eq!(visible, sorted, "ids come back ascending");
}
