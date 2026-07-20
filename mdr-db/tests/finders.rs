//! Integration tests for the import natural-key finders and the reprocess
//! delete-cascade helpers in `mdr_db::ops`.
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
            creation_date: Utc::now(),
            software_id: None,
            forcefield: None,
            forcefield_comments: None,
            temperature: None,
            is_placeholder: true,
            is_deprecated: false,
            is_public: false,
            protonation_method: None,
            fasta_sequence: None,
            alias: None,
            pdb_id: None,
            is_embargoed: false,
            is_coarse_grained: false,
            num_replicates: None,
            irods_ticket: None,
            superseding_simulation_id: None,
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

/// Set the alias / creator / file-hash keys the alias & hash finders look up.
/// Done via raw SQL because `NewSimulation` doesn't (yet) carry `created_by_id`
/// or `unique_file_hash_string` — the importer orchestration will add those.
fn set_sim_keys(
    c: &mut PgConnection,
    sim_id: i64,
    alias: Option<&str>,
    created_by: Option<i64>,
    hash: Option<&str>,
) {
    use diesel::sql_types::{BigInt, Nullable, Text};
    diesel::sql_query(
        "UPDATE md_simulation \
         SET alias = $1, created_by_id = $2, unique_file_hash_string = $3 \
         WHERE id = $4",
    )
    .bind::<Nullable<Text>, _>(alias)
    .bind::<Nullable<BigInt>, _>(created_by)
    .bind::<Nullable<Text>, _>(hash)
    .bind::<BigInt, _>(sim_id)
    .execute(c)
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
        ops::find_software_id_by_name_version(&mut c, "GROMACS-nulltest", None).unwrap(),
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
    assert_eq!(ops::find_pub_id_by_doi(&mut c, "10.1000/absent").unwrap(), None);
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
    assert_eq!(ops::find_simulation_pub_id(&mut c, sim, p + 1).unwrap(), None);
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
    assert_eq!(ops::find_simulation_id_by_hash(&mut c, "nope").unwrap(), None);
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
    assert!(ops::get_processed_file(&mut c, pf).is_err(), "target file gone");
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
    assert!(ops::get_uploaded_file(&mut c, uf).is_err(), "target file gone");
    assert!(
        ops::get_uploaded_file(&mut c, other_uf).is_ok(),
        "other sim's file untouched"
    );
    let (links, _) =
        ops::list_download_uploaded_files(&mut c, Some(di), None, true, None, None)
            .unwrap();
    assert_eq!(links, 0, "download link removed");
}
