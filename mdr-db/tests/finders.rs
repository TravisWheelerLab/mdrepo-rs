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
use mdr_db::schema::md_simulation_creator;

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
            contributor_id: None,
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
            // md_processed_file.file_type is a foreign key to
            // md_processed_file_type (md-repo-app migration 0280).
            file_type: "Processed topology".into(),
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
            // md_uploaded_file.file_type is a foreign key to
            // md_uploaded_file_type (md-repo-app migration 0281).
            file_type: "Topology".into(),
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
            response: None,
            entry_version: None,
            fetched_at: None,
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
            contributor_id: Some(created_by),
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
fn upsert_creator_dedups_on_the_coalesced_identity() {
    let mut c = conn_or_skip!();

    let mk = |name: &str, orcid: Option<&str>, inst: Option<&str>| NewCreator {
        name: Some(name.into()),
        orcid: orcid.map(Into::into),
        email: None,
        institution: inst.map(Into::into),
    };

    let first = ops::upsert_creator(
        &mut c,
        mk("Ada Lovelace", Some("0000-1"), Some("Analytical Engine Co")),
    )
    .unwrap();

    // Same identity again is the same row, not a second one.
    assert_eq!(
        ops::upsert_creator(
            &mut c,
            mk("Ada Lovelace", Some("0000-1"), Some("Analytical Engine Co"))
        )
        .unwrap(),
        first,
        "an identical identity must reuse the existing creator"
    );

    // Case-insensitive on the text fields, matching uniq_creator_identity.
    assert_eq!(
        ops::upsert_creator(
            &mut c,
            mk("ada lovelace", Some("0000-1"), Some("ANALYTICAL ENGINE CO"))
        )
        .unwrap(),
        first,
        "the match lower-cases name and institution"
    );

    // This is the case the whole NULL-safe key exists for: a stored NULL and
    // an incoming "" are the same person. Matching on the raw columns would
    // insert a duplicate here and the unique index would then reject it.
    let null_inst =
        ops::upsert_creator(&mut c, mk("Grace Hopper", Some("0000-2"), None)).unwrap();
    assert_eq!(
        ops::upsert_creator(&mut c, mk("Grace Hopper", Some("0000-2"), Some("")))
            .unwrap(),
        null_inst,
        "a NULL institution and an empty one are one identity"
    );

    // A different institution is a different creator -- best-effort matching
    // does not try to merge spellings of one human.
    assert_ne!(
        ops::upsert_creator(
            &mut c,
            mk("Ada Lovelace", Some("0000-1"), Some("Somewhere Else"))
        )
        .unwrap(),
        first,
        "identity includes institution; two spellings stay two rows"
    );
}

#[test]
fn creators_for_a_simulation_come_back_in_author_order() {
    // The bug this replaced: mdr-export read md_contribution ordered by
    // `id DESC`, so exported author lists ran BACKWARDS -- 82,143 of 104,122
    // simulations. Insertion order here is deliberately neither rank order
    // nor its reverse, so a query that forgot to order at all, or ordered by
    // id either way, fails.
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);

    let mk = |c: &mut PgConnection, name: &str| {
        ops::upsert_creator(
            c,
            NewCreator {
                name: Some(name.into()),
                orcid: None,
                email: None,
                institution: None,
            },
        )
        .unwrap()
    };
    let second = mk(&mut c, "Second Author");
    let third = mk(&mut c, "Third Author");
    let first = mk(&mut c, "First Author");

    for (creator, rank) in [(second, 2), (third, 3), (first, 1)] {
        ops::upsert_simulation_creator(
            &mut c,
            NewSimulationCreator {
                simulation_id: sim,
                creator_id: creator,
                rank,
            },
        )
        .unwrap();
    }

    let names: Vec<String> = ops::list_creators_for_simulation(&mut c, sim)
        .unwrap()
        .into_iter()
        .map(|r| r.name.unwrap_or_default())
        .collect();

    assert_eq!(
        names,
        vec!["First Author", "Second Author", "Third Author"],
        "creators must come back by rank, not by id"
    );
}

#[test]
fn creators_for_a_simulation_are_scoped_and_stable() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);
    let other = seed_sim(&mut c);

    let ada = ops::upsert_creator(
        &mut c,
        NewCreator {
            name: Some("Ada Lovelace".into()),
            orcid: Some("0000-9-ada".into()),
            email: None,
            institution: None,
        },
    )
    .unwrap();
    ops::upsert_simulation_creator(
        &mut c,
        NewSimulationCreator {
            simulation_id: sim,
            creator_id: ada,
            rank: 1,
        },
    )
    .unwrap();

    assert_eq!(
        ops::list_creators_for_simulation(&mut c, sim)
            .unwrap()
            .len(),
        1
    );
    assert!(
        ops::list_creators_for_simulation(&mut c, other)
            .unwrap()
            .is_empty(),
        "a creator linked to one simulation must not appear under another"
    );
}

#[test]
fn upsert_simulation_creator_relinks_rather_than_duplicating() {
    let mut c = conn_or_skip!();
    let sim = seed_sim(&mut c);

    let creator = ops::upsert_creator(
        &mut c,
        NewCreator {
            name: Some("Ada Lovelace".into()),
            orcid: Some("0000-3".into()),
            email: None,
            institution: None,
        },
    )
    .unwrap();

    let link = |c: &mut PgConnection, rank: i32| {
        ops::upsert_simulation_creator(
            c,
            NewSimulationCreator {
                simulation_id: sim,
                creator_id: creator,
                rank,
            },
        )
        .unwrap()
    };

    link(&mut c, 1);
    // A reprocess that reorders authors must move the rank, not add a row --
    // (simulation_id, creator_id) is the primary key.
    link(&mut c, 3);

    let rows: Vec<SimulationCreator> = md_simulation_creator::table
        .filter(md_simulation_creator::simulation_id.eq(sim))
        .select(SimulationCreator::as_select())
        .load(&mut c)
        .unwrap();

    assert_eq!(rows.len(), 1, "re-linking must not duplicate the row");
    assert_eq!(rows[0].rank, 3, "the rank must be updated in place");

    assert_eq!(
        ops::delete_simulation_creators(&mut c, sim).unwrap(),
        1,
        "the reprocess helper clears the simulation's links"
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
            smiles: Some("C1=NC2=C(C(=N1)N)N=CN2".into()),
            inchi: None,
            inchikey: None,
            declared_identity: Some("smiles".into()),
            identity_software: None,
            simulation_id: sim,
            chain_id: None,
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
            response: None,
            entry_version: None,
            fetched_at: None,
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
            response: None,
            entry_version: None,
            fetched_at: None,
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
            response: None,
            entities_response: None,
            fetched_at: None,
        },
    )
    .unwrap();

    let second = ops::upsert_pdb(
        &mut c,
        NewPdb {
            pdb_id: "1v4t-upserttest".into(),
            classification: Some("TRANSFERASE, refreshed".into()),
            title: Some("Glucokinase, refreshed".into()),
            response: None,
            entities_response: None,
            fetched_at: None,
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

/// A refresh without a fetch (mdr-process before Phase 2, or any caller with
/// no response) must not wipe the stored raw response; a fresh one replaces it.
#[test]
fn upserts_keep_a_stored_response_unless_given_a_new_one() {
    let mut c = conn_or_skip!();
    let fetched = Utc::now();
    let uni =
        |name: &str, response: Option<serde_json::Value>, entry_version| NewUniprot {
            uniprot_id: "P35557-responsetest".into(),
            name: name.into(),
            amino_length: 4,
            sequence: "MLDD".into(),
            fetched_at: response.as_ref().map(|_| fetched),
            response,
            entry_version,
        };

    ops::upsert_uniprot(&mut c, uni("a", Some(serde_json::json!({"v": 1})), Some(7)))
        .unwrap();
    let kept = ops::upsert_uniprot(&mut c, uni("b", None, None)).unwrap();
    assert_eq!(kept.name, "b");
    assert_eq!(kept.response, Some(serde_json::json!({"v": 1})));
    assert_eq!(kept.entry_version, Some(7));
    assert!(kept.fetched_at.is_some());
    let replaced = ops::upsert_uniprot(
        &mut c,
        uni("c", Some(serde_json::json!({"v": 2})), Some(8)),
    )
    .unwrap();
    assert_eq!(replaced.response, Some(serde_json::json!({"v": 2})));
    assert_eq!(replaced.entry_version, Some(8));

    let pdb = |title: &str, molecules: Option<serde_json::Value>| NewPdb {
        pdb_id: "1v4t-responsetest".into(),
        classification: None,
        title: Some(title.into()),
        fetched_at: molecules.as_ref().map(|_| fetched),
        entities_response: molecules.as_ref().map(|_| serde_json::json!({"sifts": 1})),
        response: molecules,
    };
    ops::upsert_pdb(&mut c, pdb("a", Some(serde_json::json!({"molecules": 1}))))
        .unwrap();
    let kept = ops::upsert_pdb(&mut c, pdb("b", None)).unwrap();
    assert_eq!(kept.title.as_deref(), Some("b"));
    assert_eq!(kept.response, Some(serde_json::json!({"molecules": 1})));
    assert_eq!(
        kept.entities_response,
        Some(serde_json::json!({"sifts": 1}))
    );
    assert!(kept.fetched_at.is_some());
}

/// A polymer starts with no reference; `set_polymer_reference` writes the
/// whole hit, and md_polymer_uniprot_hit refuses one past the chain's end.
#[test]
fn set_polymer_reference_writes_the_hit() {
    let mut c = conn_or_skip!();
    let uniprot = ops::insert_uniprot(
        &mut c,
        NewUniprot {
            uniprot_id: "P04585-polymertest".into(),
            name: "Gag-Pol".into(),
            amino_length: 1435,
            sequence: "M".into(),
            response: None,
            entry_version: None,
            fetched_at: None,
        },
    )
    .unwrap()
    .id;
    let residues: Vec<String> = ["PRO", "GLN", "ILE", "THR", "LEU"]
        .iter()
        .map(|r| r.to_string())
        .collect();
    let polymer = ops::find_or_insert_polymer(
        &mut c,
        &NewPolymer {
            polymer_type: "protein".into(),
            sequence: "PQITL".into(),
            residues,
            residues_hash: "ab".repeat(32),
            num_residues: 5,
        },
    )
    .unwrap();
    assert_eq!(ops::get_polymer(&mut c, polymer).unwrap().uniprot_id, None);

    let hit = PolymerReference {
        uniprot_id: uniprot,
        query_start: 1,
        query_end: 5,
        reference_start: 489,
        reference_end: 493,
        identity: 100.0,
    };
    let set = ops::set_polymer_reference(&mut c, polymer, &hit).unwrap();
    assert_eq!(set.uniprot_id, Some(uniprot));
    assert_eq!((set.query_start, set.query_end), (Some(1), Some(5)));
    assert_eq!(
        (set.reference_start, set.reference_end),
        (Some(489), Some(493))
    );
    assert_eq!(set.identity, Some(100.0));

    // Last statement: the CHECK failure aborts the transaction.
    let past_the_end = PolymerReference {
        query_end: 6,
        ..hit
    };
    assert!(ops::set_polymer_reference(&mut c, polymer, &past_the_end).is_err());
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
            response: None,
            entry_version: None,
            fetched_at: None,
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
            response: None,
            entities_response: None,
            fetched_at: None,
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

/// Run `upsert` for the same new row in two real, overlapping transactions, the
/// way two landings of one ticket import at once (MDR-37). A upserts and keeps
/// its transaction open; B upserts the same row and must block on A; only once
/// Postgres shows B waiting on a lock does A commit. Returns A's id and B's
/// result. Committed data, so `cleanup` deletes the row afterwards.
///
/// Unlike the rest of this file these tests COMMIT, because the race only
/// exists between two real transactions. Each uses a key no other test uses.
fn race_two_transactions(
    url: &str,
    upsert: fn(&mut PgConnection) -> QueryResult<i64>,
    cleanup: &str,
) -> (i64, QueryResult<i64>) {
    use diesel::sql_types::{BigInt, Integer, Nullable, Text};

    #[derive(QueryableByName)]
    struct Pid {
        #[diesel(sql_type = Integer)]
        pid: i32,
    }
    #[derive(QueryableByName)]
    struct Wait {
        #[diesel(sql_type = Nullable<Text>)]
        wait_event_type: Option<String>,
    }

    let mut a = PgConnection::establish(url).expect("connect a");
    let mut b = PgConnection::establish(url).expect("connect b");
    let mut watch = PgConnection::establish(url).expect("connect watcher");

    let b_pid = diesel::sql_query("SELECT pg_backend_pid() AS pid")
        .get_result::<Pid>(&mut b)
        .unwrap()
        .pid;

    diesel::sql_query("BEGIN").execute(&mut a).unwrap();
    let id_a = upsert(&mut a).unwrap();

    let b_thread = std::thread::spawn(move || {
        diesel::sql_query("BEGIN").execute(&mut b).unwrap();
        let r = upsert(&mut b);
        let end = if r.is_ok() { "COMMIT" } else { "ROLLBACK" };
        diesel::sql_query(end).execute(&mut b).unwrap();
        r
    });

    // Don't commit A until B is waiting on a lock; otherwise B could run after
    // A commits and pass without exercising the race.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let w = diesel::sql_query(
            "SELECT wait_event_type FROM pg_stat_activity WHERE pid = $1",
        )
        .bind::<Integer, _>(b_pid)
        .get_result::<Wait>(&mut watch)
        .unwrap();
        if w.wait_event_type.as_deref() == Some("Lock") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "B never blocked on A's insert"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    diesel::sql_query("COMMIT").execute(&mut a).unwrap();
    let id_b = b_thread.join().unwrap();

    diesel::sql_query(cleanup)
        .bind::<BigInt, _>(id_a)
        .execute(&mut watch)
        .unwrap();

    (id_a, id_b)
}

/// A new creator, imported by two landings at once, resolves to one row and
/// neither landing fails on `uniq_creator_identity`.
#[test]
fn upsert_creator_waits_for_a_concurrent_insert_instead_of_failing() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("skipping: TEST_DATABASE_URL is unset");
        return;
    };
    let (id_a, id_b) = race_two_transactions(
        &url,
        |c| {
            ops::upsert_creator(
                c,
                NewCreator {
                    name: Some(format!("MDR-37 creator race {}", std::process::id())),
                    orcid: None,
                    email: None,
                    institution: None,
                },
            )
        },
        "DELETE FROM md_creator WHERE id = $1",
    );
    assert_eq!(
        id_b.expect("B must not fail on uniq_creator_identity"),
        id_a
    );
}

/// A new paper with a DOI, cited by two landings at once, resolves to one row
/// and neither landing fails on `md_repo_app_pub_doi_key`.
#[test]
fn upsert_pub_by_doi_waits_for_a_concurrent_insert_instead_of_failing() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("skipping: TEST_DATABASE_URL is unset");
        return;
    };
    let (id_a, id_b) = race_two_transactions(
        &url,
        |c| {
            ops::upsert_pub_by_doi(
                c,
                NewPub {
                    title: "A race".into(),
                    authors: "A. Author".into(),
                    journal: "J. Tests".into(),
                    volume: 1,
                    number: None,
                    year: 2026,
                    pages: None,
                    doi: Some(format!(
                        "10.0000/mdr-37-pub-race-{}",
                        std::process::id()
                    )),
                },
            )
        },
        "DELETE FROM md_pub WHERE id = $1",
    );
    assert_eq!(
        id_b.expect("B must not fail on md_repo_app_pub_doi_key"),
        id_a
    );
}

/// The lookup must lowercase the way the index does. Rust lowercases 'İ'
/// (U+0130) to "i\u{307}" but Postgres to "i", so a Rust-side lowercase never
/// matched a stored name containing it: the second import of such a creator
/// missed the row and failed on `uniq_creator_identity`.
#[test]
fn upsert_creator_finds_a_name_that_rust_and_postgres_lowercase_differently() {
    let mut c = conn_or_skip!();
    assert_ne!(
        "İ".to_lowercase(),
        "i",
        "the premise: Rust differs from Postgres here"
    );

    let mk = || NewCreator {
        name: Some("Özgür İlkay Şahin".into()),
        orcid: None,
        email: Some("OZGUR@EXAMPLE.ORG".into()),
        institution: None,
    };
    let first = ops::upsert_creator(&mut c, mk()).unwrap();
    let again = ops::upsert_creator(&mut c, mk()).unwrap();
    assert_eq!(first, again);
}
