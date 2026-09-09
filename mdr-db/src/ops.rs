use diesel::dsl::count_star;
use diesel::pg::Pg;
use diesel::prelude::*;

use crate::models::*;
use crate::schema::*;

// ── helpers ───────────────────────────────────────────────────────────────────

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 1000;

fn limit(n: Option<i64>) -> i64 {
    n.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT)
}

// ── md_collection ────────────────────────────────────────────────────────────

fn collection_query(search: Option<&str>) -> md_collection::BoxedQuery<'static, Pg> {
    use crate::schema::md_collection::dsl::*;
    let mut q = md_collection.into_boxed();
    if let Some(t) = search {
        q = q.filter(name.ilike(format!("%{t}%")));
    }
    q
}

pub fn list_collections(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Collection>)> {
    use crate::schema::md_collection::dsl::id;
    let count = collection_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = collection_query(search.as_deref())
        .order(id.desc())
        .select(Collection::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_collection(conn: &mut PgConnection, rid: i64) -> QueryResult<Collection> {
    md_collection::table
        .find(rid)
        .select(Collection::as_select())
        .first(conn)
}

pub fn insert_collection(
    conn: &mut PgConnection,
    new: NewCollection,
) -> QueryResult<Collection> {
    diesel::insert_into(md_collection::table)
        .values(&new)
        .returning(Collection::as_returning())
        .get_result(conn)
}

pub fn update_collection(
    conn: &mut PgConnection,
    rid: i64,
    cs: CollectionUpdate,
) -> QueryResult<Collection> {
    diesel::update(md_collection::table.find(rid))
        .set(&cs)
        .returning(Collection::as_returning())
        .get_result(conn)
}

pub fn delete_collection(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_collection::table.find(rid)).execute(conn)
}

/// Find or create the `(user_id, name)` collection, without touching its
/// `description`. Fast path (a plain `SELECT`) avoids taking a write lock --
/// and re-firing any future trigger on the row -- for the common case where
/// the collection already exists; only a brand-new `(user_id, name)` pair
/// falls through to the slow path's self-assigning `ON CONFLICT DO UPDATE`.
/// See `upsert_software` for why that self-assignment, rather than a
/// SELECT-only insert, is still the right shape for the race-safety net.
pub fn upsert_collection(
    conn: &mut PgConnection,
    new: NewCollection,
) -> QueryResult<Collection> {
    use diesel::OptionalExtension;
    use diesel::upsert::excluded;

    if let Some(found) = md_collection::table
        .filter(md_collection::user_id.eq(new.user_id))
        .filter(md_collection::name.eq(&new.name))
        .select(Collection::as_select())
        .first(conn)
        .optional()?
    {
        return Ok(found);
    }

    diesel::insert_into(md_collection::table)
        .values(&new)
        .on_conflict((md_collection::user_id, md_collection::name))
        .do_update()
        .set(md_collection::name.eq(excluded(md_collection::name)))
        .returning(Collection::as_returning())
        .get_result(conn)
}

// ── md_contribution ───────────────────────────────────────────────────────────

fn contribution_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_contribution::BoxedQuery<'static, Pg> {
    use crate::schema::md_contribution::dsl::*;
    let mut q = md_contribution.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(email.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_contributions(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Contribution>)> {
    use crate::schema::md_contribution::dsl::id;
    let count = contribution_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = contribution_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(Contribution::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_contribution(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<Contribution> {
    md_contribution::table
        .find(rid)
        .select(Contribution::as_select())
        .first(conn)
}

pub fn insert_contribution(
    conn: &mut PgConnection,
    new: NewContribution,
) -> QueryResult<Contribution> {
    diesel::insert_into(md_contribution::table)
        .values(&new)
        .returning(Contribution::as_returning())
        .get_result(conn)
}

pub fn update_contribution(
    conn: &mut PgConnection,
    rid: i64,
    cs: ContributionUpdate,
) -> QueryResult<Contribution> {
    diesel::update(md_contribution::table.find(rid))
        .set(&cs)
        .returning(Contribution::as_returning())
        .get_result(conn)
}

pub fn delete_contribution(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_contribution::table.find(rid)).execute(conn)
}

// ── md_external_link ──────────────────────────────────────────────────────────

fn external_link_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_external_link::BoxedQuery<'static, Pg> {
    use crate::schema::md_external_link::dsl::*;
    let mut q = md_external_link.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(url.ilike(p.clone()).or(label.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_external_links(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<ExternalLink>)> {
    use crate::schema::md_external_link::dsl::id;
    let count = external_link_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = external_link_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(ExternalLink::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_external_link(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<ExternalLink> {
    md_external_link::table
        .find(rid)
        .select(ExternalLink::as_select())
        .first(conn)
}

pub fn insert_external_link(
    conn: &mut PgConnection,
    new: NewExternalLink,
) -> QueryResult<ExternalLink> {
    diesel::insert_into(md_external_link::table)
        .values(&new)
        .returning(ExternalLink::as_returning())
        .get_result(conn)
}

pub fn update_external_link(
    conn: &mut PgConnection,
    rid: i64,
    cs: ExternalLinkUpdate,
) -> QueryResult<ExternalLink> {
    diesel::update(md_external_link::table.find(rid))
        .set(&cs)
        .returning(ExternalLink::as_returning())
        .get_result(conn)
}

pub fn delete_external_link(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_external_link::table.find(rid)).execute(conn)
}

// ── md_feature_switch ─────────────────────────────────────────────────────────

pub fn list_feature_switches(
    conn: &mut PgConnection,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<FeatureSwitch>)> {
    use crate::schema::md_feature_switch::dsl::*;
    let count: i64 = md_feature_switch.count().get_result(conn)?;
    let mut q = md_feature_switch
        .order(id.desc())
        .select(FeatureSwitch::as_select())
        .into_boxed();
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_feature_switch(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<FeatureSwitch> {
    md_feature_switch::table
        .find(rid)
        .select(FeatureSwitch::as_select())
        .first(conn)
}

pub fn insert_feature_switch(
    conn: &mut PgConnection,
    new: NewFeatureSwitch,
) -> QueryResult<FeatureSwitch> {
    diesel::insert_into(md_feature_switch::table)
        .values(&new)
        .returning(FeatureSwitch::as_returning())
        .get_result(conn)
}

pub fn update_feature_switch(
    conn: &mut PgConnection,
    rid: i64,
    cs: FeatureSwitchUpdate,
) -> QueryResult<FeatureSwitch> {
    diesel::update(md_feature_switch::table.find(rid))
        .set(&cs)
        .returning(FeatureSwitch::as_returning())
        .get_result(conn)
}

pub fn delete_feature_switch(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_feature_switch::table.find(rid)).execute(conn)
}

// ── md_frontend_download_instance ────────────────────────────────────────────

fn download_instance_query(
    sim_id: Option<i64>,
    uid: Option<i64>,
) -> md_frontend_download_instance::BoxedQuery<'static, Pg> {
    use crate::schema::md_frontend_download_instance::dsl::*;
    let mut q = md_frontend_download_instance.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(u) = uid {
        q = q.filter(user_id.eq(u));
    }
    q
}

pub fn list_download_instances(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    uid: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<DownloadInstance>)> {
    use crate::schema::md_frontend_download_instance::dsl::id;
    let count = download_instance_query(sim_id, uid)
        .select(count_star())
        .first(conn)?;
    let mut q = download_instance_query(sim_id, uid)
        .order(id.desc())
        .select(DownloadInstance::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_download_instance(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<DownloadInstance> {
    md_frontend_download_instance::table
        .find(rid)
        .select(DownloadInstance::as_select())
        .first(conn)
}

pub fn insert_download_instance(
    conn: &mut PgConnection,
    new: NewDownloadInstance,
) -> QueryResult<DownloadInstance> {
    diesel::insert_into(md_frontend_download_instance::table)
        .values(&new)
        .returning(DownloadInstance::as_returning())
        .get_result(conn)
}

pub fn update_download_instance(
    conn: &mut PgConnection,
    rid: i64,
    cs: DownloadInstanceUpdate,
) -> QueryResult<DownloadInstance> {
    diesel::update(md_frontend_download_instance::table.find(rid))
        .set(&cs)
        .returning(DownloadInstance::as_returning())
        .get_result(conn)
}

pub fn delete_download_instance(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<usize> {
    diesel::delete(md_frontend_download_instance::table.find(rid)).execute(conn)
}

// ── md_frontend_download_instance_processed_files ────────────────────────────

fn download_processed_file_query(
    di_id: Option<i64>,
    pf_id: Option<i64>,
) -> md_frontend_download_instance_processed_files::BoxedQuery<'static, Pg> {
    use crate::schema::md_frontend_download_instance_processed_files::dsl::*;
    let mut q = md_frontend_download_instance_processed_files.into_boxed();
    if let Some(v) = di_id {
        q = q.filter(frontenddownloadinstance_id.eq(v));
    }
    if let Some(v) = pf_id {
        q = q.filter(simulationprocessedfile_id.eq(v));
    }
    q
}

pub fn list_download_processed_files(
    conn: &mut PgConnection,
    di_id: Option<i64>,
    pf_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<DownloadProcessedFile>)> {
    use crate::schema::md_frontend_download_instance_processed_files::dsl::id;
    let count = download_processed_file_query(di_id, pf_id)
        .select(count_star())
        .first(conn)?;
    let mut q = download_processed_file_query(di_id, pf_id)
        .order(id.desc())
        .select(DownloadProcessedFile::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_download_processed_file(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<DownloadProcessedFile> {
    md_frontend_download_instance_processed_files::table
        .find(rid)
        .select(DownloadProcessedFile::as_select())
        .first(conn)
}

pub fn insert_download_processed_file(
    conn: &mut PgConnection,
    new: NewDownloadProcessedFile,
) -> QueryResult<DownloadProcessedFile> {
    diesel::insert_into(md_frontend_download_instance_processed_files::table)
        .values(&new)
        .returning(DownloadProcessedFile::as_returning())
        .get_result(conn)
}

pub fn update_download_processed_file(
    conn: &mut PgConnection,
    rid: i64,
    cs: DownloadProcessedFileUpdate,
) -> QueryResult<DownloadProcessedFile> {
    diesel::update(md_frontend_download_instance_processed_files::table.find(rid))
        .set(&cs)
        .returning(DownloadProcessedFile::as_returning())
        .get_result(conn)
}

pub fn delete_download_processed_file(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<usize> {
    diesel::delete(md_frontend_download_instance_processed_files::table.find(rid))
        .execute(conn)
}

// ── md_frontend_download_instance_uploaded_files ─────────────────────────────

fn download_uploaded_file_query(
    di_id: Option<i64>,
    uf_id: Option<i64>,
) -> md_frontend_download_instance_uploaded_files::BoxedQuery<'static, Pg> {
    use crate::schema::md_frontend_download_instance_uploaded_files::dsl::*;
    let mut q = md_frontend_download_instance_uploaded_files.into_boxed();
    if let Some(v) = di_id {
        q = q.filter(frontenddownloadinstance_id.eq(v));
    }
    if let Some(v) = uf_id {
        q = q.filter(simulationuploadedfile_id.eq(v));
    }
    q
}

pub fn list_download_uploaded_files(
    conn: &mut PgConnection,
    di_id: Option<i64>,
    uf_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<DownloadUploadedFile>)> {
    use crate::schema::md_frontend_download_instance_uploaded_files::dsl::id;
    let count = download_uploaded_file_query(di_id, uf_id)
        .select(count_star())
        .first(conn)?;
    let mut q = download_uploaded_file_query(di_id, uf_id)
        .order(id.desc())
        .select(DownloadUploadedFile::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_download_uploaded_file(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<DownloadUploadedFile> {
    md_frontend_download_instance_uploaded_files::table
        .find(rid)
        .select(DownloadUploadedFile::as_select())
        .first(conn)
}

pub fn insert_download_uploaded_file(
    conn: &mut PgConnection,
    new: NewDownloadUploadedFile,
) -> QueryResult<DownloadUploadedFile> {
    diesel::insert_into(md_frontend_download_instance_uploaded_files::table)
        .values(&new)
        .returning(DownloadUploadedFile::as_returning())
        .get_result(conn)
}

pub fn update_download_uploaded_file(
    conn: &mut PgConnection,
    rid: i64,
    cs: DownloadUploadedFileUpdate,
) -> QueryResult<DownloadUploadedFile> {
    diesel::update(md_frontend_download_instance_uploaded_files::table.find(rid))
        .set(&cs)
        .returning(DownloadUploadedFile::as_returning())
        .get_result(conn)
}

pub fn delete_download_uploaded_file(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<usize> {
    diesel::delete(md_frontend_download_instance_uploaded_files::table.find(rid))
        .execute(conn)
}

// ── md_ligand ─────────────────────────────────────────────────────────────────

fn ligand_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_ligand::BoxedQuery<'static, Pg> {
    use crate::schema::md_ligand::dsl::*;
    let mut q = md_ligand.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(smiles.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_ligands(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Ligand>)> {
    use crate::schema::md_ligand::dsl::id;
    let count = ligand_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = ligand_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(Ligand::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_ligand(conn: &mut PgConnection, rid: i64) -> QueryResult<Ligand> {
    md_ligand::table
        .find(rid)
        .select(Ligand::as_select())
        .first(conn)
}

pub fn insert_ligand(conn: &mut PgConnection, new: NewLigand) -> QueryResult<Ligand> {
    diesel::insert_into(md_ligand::table)
        .values(&new)
        .returning(Ligand::as_returning())
        .get_result(conn)
}

pub fn update_ligand(
    conn: &mut PgConnection,
    rid: i64,
    cs: LigandUpdate,
) -> QueryResult<Ligand> {
    diesel::update(md_ligand::table.find(rid))
        .set(&cs)
        .returning(Ligand::as_returning())
        .get_result(conn)
}

pub fn delete_ligand(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_ligand::table.find(rid)).execute(conn)
}

// ── md_pdb ────────────────────────────────────────────────────────────────────

fn pdb_query(search: Option<&str>) -> md_pdb::BoxedQuery<'static, Pg> {
    use crate::schema::md_pdb::dsl::*;
    let mut q = md_pdb.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(
            pdb_id
                .ilike(p.clone())
                .or(title.ilike(p.clone()))
                .or(classification.ilike(p)),
        );
    }
    q
}

pub fn list_pdbs(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Pdb>)> {
    use crate::schema::md_pdb::dsl::id;
    let count = pdb_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = pdb_query(search.as_deref())
        .order(id.desc())
        .select(Pdb::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_pdb(conn: &mut PgConnection, rid: i64) -> QueryResult<Pdb> {
    md_pdb::table.find(rid).select(Pdb::as_select()).first(conn)
}

pub fn insert_pdb(conn: &mut PgConnection, new: NewPdb) -> QueryResult<Pdb> {
    diesel::insert_into(md_pdb::table)
        .values(&new)
        .returning(Pdb::as_returning())
        .get_result(conn)
}

/// Insert the PDB code, or refresh the existing row, in one statement.
///
/// Same race as `upsert_uniprot`, same reason: md_pdb rows are shared across
/// simulations and landing directories are processed in parallel, so two
/// threads citing the same new code both find nothing and both insert, and
/// md_pdb has UNIQUE (pdb_id) to turn that into a failed directory. Fixed
/// alongside the uniprot one on 2026-08-10, before it was ever observed --
/// ticket 2175 was 16 variants of a single structure, which is exactly the
/// shape that produced the uniprot collision.
///
/// Callers lowercase the code first; that is not done here, so this upserts
/// on whatever it is given.
pub fn upsert_pdb(conn: &mut PgConnection, new: NewPdb) -> QueryResult<Pdb> {
    use diesel::upsert::excluded;

    diesel::insert_into(md_pdb::table)
        .values(&new)
        .on_conflict(md_pdb::pdb_id)
        .do_update()
        .set((
            md_pdb::classification.eq(excluded(md_pdb::classification)),
            md_pdb::title.eq(excluded(md_pdb::title)),
        ))
        .returning(Pdb::as_returning())
        .get_result(conn)
}

pub fn update_pdb(
    conn: &mut PgConnection,
    rid: i64,
    cs: PdbUpdate,
) -> QueryResult<Pdb> {
    diesel::update(md_pdb::table.find(rid))
        .set(&cs)
        .returning(Pdb::as_returning())
        .get_result(conn)
}

pub fn delete_pdb(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_pdb::table.find(rid)).execute(conn)
}

// ── md_processed_file ─────────────────────────────────────────────────────────

fn processed_file_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_processed_file::BoxedQuery<'static, Pg> {
    use crate::schema::md_processed_file::dsl::*;
    let mut q = md_processed_file.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(filename.ilike(p.clone()).or(file_type.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_processed_files(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<ProcessedFile>)> {
    use crate::schema::md_processed_file::dsl::id;
    let count = processed_file_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = processed_file_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(ProcessedFile::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_processed_file(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<ProcessedFile> {
    md_processed_file::table
        .find(rid)
        .select(ProcessedFile::as_select())
        .first(conn)
}

pub fn insert_processed_file(
    conn: &mut PgConnection,
    new: NewProcessedFile,
) -> QueryResult<ProcessedFile> {
    diesel::insert_into(md_processed_file::table)
        .values(&new)
        .returning(ProcessedFile::as_returning())
        .get_result(conn)
}

pub fn update_processed_file(
    conn: &mut PgConnection,
    rid: i64,
    cs: ProcessedFileUpdate,
) -> QueryResult<ProcessedFile> {
    diesel::update(md_processed_file::table.find(rid))
        .set(&cs)
        .returning(ProcessedFile::as_returning())
        .get_result(conn)
}

pub fn delete_processed_file(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_processed_file::table.find(rid)).execute(conn)
}

// ── md_replicate ──────────────────────────────────────────────────────────────

fn replicate_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_replicate::BoxedQuery<'static, Pg> {
    use crate::schema::md_replicate::dsl::*;
    let mut q = md_replicate.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(trajectory_file_name.ilike(p));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_replicates(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Replicate>)> {
    use crate::schema::md_replicate::dsl::id;
    let count = replicate_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = replicate_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(Replicate::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_replicate(conn: &mut PgConnection, rid: i64) -> QueryResult<Replicate> {
    md_replicate::table
        .find(rid)
        .select(Replicate::as_select())
        .first(conn)
}

pub fn insert_replicate(
    conn: &mut PgConnection,
    new: NewReplicate,
) -> QueryResult<Replicate> {
    diesel::insert_into(md_replicate::table)
        .values(&new)
        .returning(Replicate::as_returning())
        .get_result(conn)
}

pub fn update_replicate(
    conn: &mut PgConnection,
    rid: i64,
    cs: ReplicateUpdate,
) -> QueryResult<Replicate> {
    diesel::update(md_replicate::table.find(rid))
        .set(&cs)
        .returning(Replicate::as_returning())
        .get_result(conn)
}

pub fn delete_replicate(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_replicate::table.find(rid)).execute(conn)
}

// ── md_pub ────────────────────────────────────────────────────────────────────

fn pub_query(search: Option<&str>) -> md_pub::BoxedQuery<'static, Pg> {
    use crate::schema::md_pub::dsl::*;
    let mut q = md_pub.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(
            title
                .ilike(p.clone())
                .or(authors.ilike(p.clone()))
                .or(journal.ilike(p)),
        );
    }
    q
}

pub fn list_pubs(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Pub>)> {
    use crate::schema::md_pub::dsl::id;
    let count = pub_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = pub_query(search.as_deref())
        .order(id.desc())
        .select(Pub::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_pub(conn: &mut PgConnection, rid: i64) -> QueryResult<Pub> {
    md_pub::table.find(rid).select(Pub::as_select()).first(conn)
}

pub fn insert_pub(conn: &mut PgConnection, new: NewPub) -> QueryResult<Pub> {
    diesel::insert_into(md_pub::table)
        .values(&new)
        .returning(Pub::as_returning())
        .get_result(conn)
}

pub fn update_pub(
    conn: &mut PgConnection,
    rid: i64,
    cs: PubUpdate,
) -> QueryResult<Pub> {
    diesel::update(md_pub::table.find(rid))
        .set(&cs)
        .returning(Pub::as_returning())
        .get_result(conn)
}

pub fn delete_pub(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_pub::table.find(rid)).execute(conn)
}

// ── md_simulation ─────────────────────────────────────────────────────────────

pub fn get_all_simulation_ids(conn: &mut PgConnection) -> QueryResult<Vec<i64>> {
    use crate::schema::md_simulation::dsl::*;
    md_simulation.select(id).order(id.asc()).load(conn)
}

/// IDs of the simulations that are visible to the public, i.e. the exact set
/// the Django app's `/simulation_list` endpoint serves.
///
/// This mirrors `visible_simulations_q()` in the Django app
/// (`md_repo_app/models/simulation/simulation_filters.py`), which is the source
/// of truth for "publicly visible". All four conditions are required, and the
/// two easy ones to forget are the ones that matter: `is_public` alone is not
/// enough because embargo outlives approval, and a placeholder row is a
/// simulation whose files have not landed yet. On production this is the
/// difference between 72,998 rows and 54,601.
///
/// Note this is deliberately STRICTER than `Simulation.is_visible_to()` in the
/// same Django app, which omits `is_deprecated` so a deprecated simulation
/// stays reachable by direct link while being hidden from browse. Anything
/// built on this function inherits the list-endpoint rule, not the detail one.
///
/// Two more copies of this predicate exist in SQL, both named `VISIBLE_SIM`:
/// in `utils/python/export_mapping_file.py` and in
/// `utils/python/export_public_metadata.py`, where it re-checks visibility
/// after the export rather than selecting rows. Change one, change all four.
pub fn get_visible_simulation_ids(conn: &mut PgConnection) -> QueryResult<Vec<i64>> {
    use crate::schema::md_simulation::dsl::*;
    md_simulation
        .filter(is_public.eq(true))
        .filter(is_deprecated.eq(false))
        .filter(is_placeholder.eq(false))
        .filter(is_embargoed.eq(false))
        .select(id)
        .order(id.asc())
        .load(conn)
}

fn simulation_query(
    search: Option<&str>,
    public_only: bool,
    active: bool,
) -> md_simulation::BoxedQuery<'static, Pg> {
    use crate::schema::md_simulation::dsl::*;
    let mut q = md_simulation.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(description.ilike(p.clone()).or(short_description.ilike(p)));
    }
    if public_only {
        q = q.filter(is_public.eq(true));
    }
    if active {
        q = q.filter(is_deprecated.eq(false));
    }
    q
}

pub fn list_simulations(
    conn: &mut PgConnection,
    search: Option<String>,
    public_only: bool,
    active: bool,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Simulation>)> {
    use crate::schema::md_simulation::dsl::id;
    let count = simulation_query(search.as_deref(), public_only, active)
        .select(count_star())
        .first(conn)?;
    let mut q = simulation_query(search.as_deref(), public_only, active)
        .order(id.desc())
        .select(Simulation::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_simulation(conn: &mut PgConnection, rid: i64) -> QueryResult<Simulation> {
    md_simulation::table
        .find(rid)
        .select(Simulation::as_select())
        .first(conn)
}

pub fn insert_simulation(
    conn: &mut PgConnection,
    new: NewSimulation,
) -> QueryResult<Simulation> {
    diesel::insert_into(md_simulation::table)
        .values(&new)
        .returning(Simulation::as_returning())
        .get_result(conn)
}

pub fn update_simulation(
    conn: &mut PgConnection,
    rid: i64,
    cs: SimulationUpdate,
) -> QueryResult<Simulation> {
    diesel::update(md_simulation::table.find(rid))
        .set(&cs)
        .returning(Simulation::as_returning())
        .get_result(conn)
}

pub fn delete_simulation(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_simulation::table.find(rid)).execute(conn)
}

// ── md_simulation_pub ─────────────────────────────────────────────────────────

fn simulation_pub_query(
    sim_id: Option<i64>,
    pid: Option<i64>,
) -> md_simulation_pub::BoxedQuery<'static, Pg> {
    use crate::schema::md_simulation_pub::dsl::*;
    let mut q = md_simulation_pub.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(p) = pid {
        q = q.filter(pub_id.eq(p));
    }
    q
}

pub fn list_simulation_pubs(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    pid: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SimulationPub>)> {
    use crate::schema::md_simulation_pub::dsl::id;
    let count = simulation_pub_query(sim_id, pid)
        .select(count_star())
        .first(conn)?;
    let mut q = simulation_pub_query(sim_id, pid)
        .order(id.desc())
        .select(SimulationPub::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_simulation_pub(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<SimulationPub> {
    md_simulation_pub::table
        .find(rid)
        .select(SimulationPub::as_select())
        .first(conn)
}

pub fn insert_simulation_pub(
    conn: &mut PgConnection,
    new: NewSimulationPub,
) -> QueryResult<SimulationPub> {
    diesel::insert_into(md_simulation_pub::table)
        .values(&new)
        .returning(SimulationPub::as_returning())
        .get_result(conn)
}

pub fn update_simulation_pub(
    conn: &mut PgConnection,
    rid: i64,
    cs: SimulationPubUpdate,
) -> QueryResult<SimulationPub> {
    diesel::update(md_simulation_pub::table.find(rid))
        .set(&cs)
        .returning(SimulationPub::as_returning())
        .get_result(conn)
}

pub fn delete_simulation_pub(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_simulation_pub::table.find(rid)).execute(conn)
}

// ── md_simulation_collection ────────────────────────────────────────────────────

fn simulation_collection_query(
    sim_id: Option<i64>,
    cid: Option<i64>,
) -> md_simulation_collection::BoxedQuery<'static, Pg> {
    use crate::schema::md_simulation_collection::dsl::*;
    let mut q = md_simulation_collection.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(c) = cid {
        q = q.filter(collection_id.eq(c));
    }
    q
}

pub fn list_simulation_collections(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    cid: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SimulationCollection>)> {
    use crate::schema::md_simulation_collection::dsl::id;
    let count = simulation_collection_query(sim_id, cid)
        .select(count_star())
        .first(conn)?;
    let mut q = simulation_collection_query(sim_id, cid)
        .order(id.desc())
        .select(SimulationCollection::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_simulation_collection(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<SimulationCollection> {
    md_simulation_collection::table
        .find(rid)
        .select(SimulationCollection::as_select())
        .first(conn)
}

pub fn insert_simulation_collection(
    conn: &mut PgConnection,
    new: NewSimulationCollection,
) -> QueryResult<SimulationCollection> {
    diesel::insert_into(md_simulation_collection::table)
        .values(&new)
        .returning(SimulationCollection::as_returning())
        .get_result(conn)
}

pub fn update_simulation_collection(
    conn: &mut PgConnection,
    rid: i64,
    cs: SimulationCollectionUpdate,
) -> QueryResult<SimulationCollection> {
    diesel::update(md_simulation_collection::table.find(rid))
        .set(&cs)
        .returning(SimulationCollection::as_returning())
        .get_result(conn)
}

pub fn delete_simulation_collection(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<usize> {
    diesel::delete(md_simulation_collection::table.find(rid)).execute(conn)
}

/// Existing `md_simulation_collection` link id for `(simulation_id,
/// collection_id)`, so the importer doesn't create a duplicate link (mirrors
/// `find_simulation_uniprot_id`).
pub fn find_simulation_collection_id(
    conn: &mut PgConnection,
    sim_id: i64,
    cid: i64,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_simulation_collection::dsl::*;
    md_simulation_collection
        .filter(simulation_id.eq(sim_id))
        .filter(collection_id.eq(cid))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

// ── md_simulation_uniprot ─────────────────────────────────────────────────────

fn simulation_uniprot_query(
    sim_id: Option<i64>,
    upid: Option<i64>,
) -> md_simulation_uniprot::BoxedQuery<'static, Pg> {
    use crate::schema::md_simulation_uniprot::dsl::*;
    let mut q = md_simulation_uniprot.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(u) = upid {
        q = q.filter(uniprot_id.eq(u));
    }
    q
}

pub fn list_simulation_uniprots(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    upid: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SimulationUniprot>)> {
    use crate::schema::md_simulation_uniprot::dsl::id;
    let count = simulation_uniprot_query(sim_id, upid)
        .select(count_star())
        .first(conn)?;
    let mut q = simulation_uniprot_query(sim_id, upid)
        .order(id.desc())
        .select(SimulationUniprot::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_simulation_uniprot(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<SimulationUniprot> {
    md_simulation_uniprot::table
        .find(rid)
        .select(SimulationUniprot::as_select())
        .first(conn)
}

pub fn insert_simulation_uniprot(
    conn: &mut PgConnection,
    new: NewSimulationUniprot,
) -> QueryResult<SimulationUniprot> {
    diesel::insert_into(md_simulation_uniprot::table)
        .values(&new)
        .returning(SimulationUniprot::as_returning())
        .get_result(conn)
}

pub fn update_simulation_uniprot(
    conn: &mut PgConnection,
    rid: i64,
    cs: SimulationUniprotUpdate,
) -> QueryResult<SimulationUniprot> {
    diesel::update(md_simulation_uniprot::table.find(rid))
        .set(&cs)
        .returning(SimulationUniprot::as_returning())
        .get_result(conn)
}

pub fn delete_simulation_uniprot(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<usize> {
    diesel::delete(md_simulation_uniprot::table.find(rid)).execute(conn)
}

// ── md_software ───────────────────────────────────────────────────────────────

fn software_query(search: Option<&str>) -> md_software::BoxedQuery<'static, Pg> {
    use crate::schema::md_software::dsl::*;
    let mut q = md_software.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p));
    }
    q
}

pub fn list_software(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Software>)> {
    use crate::schema::md_software::dsl::id;
    let count = software_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = software_query(search.as_deref())
        .order(id.desc())
        .select(Software::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_software(conn: &mut PgConnection, rid: i64) -> QueryResult<Software> {
    md_software::table
        .find(rid)
        .select(Software::as_select())
        .first(conn)
}

pub fn insert_software(
    conn: &mut PgConnection,
    new: NewSoftware,
) -> QueryResult<Software> {
    diesel::insert_into(md_software::table)
        .values(&new)
        .returning(Software::as_returning())
        .get_result(conn)
}

/// Insert the (name, version) pair, or return the existing row, in one
/// statement.
///
/// Third instance of the find-then-insert race fixed on 2026-08-10, after
/// md_uniprot and md_pdb. This one is the oldest and was the only one that had
/// already done damage, because md_software had no unique constraint until the
/// Django migration `0258_simulationsoftware_md_software_name_version_uniq`:
/// the losing thread did not fail, it silently inserted a second row. AMBER
/// 2022 existed twice on prod (ids 53 and 54) and split three simulations 2/1;
/// they were created 138 ms apart in one parallel batch. Merged onto id 53 the
/// same day.
///
/// **This function and that migration must ship together.** Adding the
/// constraint while callers still find-then-insert converts a silent duplicate
/// into a failed landing directory; fixing this without the constraint leaves
/// ON CONFLICT with no unique index to target, which is an error.
///
/// The SET is a self-assignment because the table has no columns beyond the
/// conflict target -- DO UPDATE is still required, since DO NOTHING returns no
/// row and get_result would fail with NotFound exactly when we lost the race.
///
/// Caveat inherited from the constraint: a NULL version does not conflict in
/// Postgres, so two (name, NULL) rows can still both insert. Neither database
/// holds a null or blank version today. See the model's Meta for the rest.
pub fn upsert_software(
    conn: &mut PgConnection,
    new: NewSoftware,
) -> QueryResult<Software> {
    use diesel::upsert::excluded;
    use diesel::{OptionalExtension, PgExpressionMethods};

    // Fast path: the row almost always exists, and SELECT neither locks it nor
    // fires triggers. The DO UPDATE arm below is a *self-assignment* of `name`,
    // which sounds free but is not: `trg_software_search_text` is defined AFTER
    // UPDATE OF name, and Postgres fires column triggers on the column being in
    // the SET list, not on the value changing. So every upsert of an existing
    // row re-ran that trigger, which loops over every md_simulation sharing the
    // software and rewrites its search_text -- 19,943 row updates against a
    // 19 GB table per import for ('AMBER','2020'). See the 2026-08-22 incident.
    if let Some(found) = md_software::table
        .filter(md_software::name.eq(&new.name))
        // IS NOT DISTINCT FROM, so a NULL version matches a NULL version
        .filter(md_software::version.is_not_distinct_from(&new.version))
        .select(Software::as_select())
        .first(conn)
        .optional()?
    {
        return Ok(found);
    }

    // Slow path: first sighting of this (name, version). The trigger may fire
    // here, but a software row this new has no simulations pointing at it yet.
    diesel::insert_into(md_software::table)
        .values(&new)
        .on_conflict((md_software::name, md_software::version))
        .do_update()
        .set(md_software::name.eq(excluded(md_software::name)))
        .returning(Software::as_returning())
        .get_result(conn)
}

pub fn update_software(
    conn: &mut PgConnection,
    rid: i64,
    cs: SoftwareUpdate,
) -> QueryResult<Software> {
    diesel::update(md_software::table.find(rid))
        .set(&cs)
        .returning(Software::as_returning())
        .get_result(conn)
}

pub fn delete_software(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_software::table.find(rid)).execute(conn)
}

// ── md_solute ────────────────────────────────────────────────────────────────

fn solute_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_solute::BoxedQuery<'static, Pg> {
    use crate::schema::md_solute::dsl::*;
    let mut q = md_solute.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_solutes(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Solute>)> {
    use crate::schema::md_solute::dsl::id;
    let count = solute_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = solute_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(Solute::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_solute(conn: &mut PgConnection, rid: i64) -> QueryResult<Solute> {
    md_solute::table
        .find(rid)
        .select(Solute::as_select())
        .first(conn)
}

pub fn insert_solute(conn: &mut PgConnection, new: NewSolute) -> QueryResult<Solute> {
    diesel::insert_into(md_solute::table)
        .values(&new)
        .returning(Solute::as_returning())
        .get_result(conn)
}

pub fn update_solute(
    conn: &mut PgConnection,
    rid: i64,
    cs: SoluteUpdate,
) -> QueryResult<Solute> {
    diesel::update(md_solute::table.find(rid))
        .set(&cs)
        .returning(Solute::as_returning())
        .get_result(conn)
}

pub fn delete_solute(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_solute::table.find(rid)).execute(conn)
}

// ── md_submission_completed_event ─────────────────────────────────────────────

fn submission_event_query(
    search: Option<&str>,
) -> md_submission_completed_event::BoxedQuery<'static, Pg> {
    use crate::schema::md_submission_completed_event::dsl::*;
    let mut q = md_submission_completed_event.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(path.ilike(p));
    }
    q
}

pub fn list_submission_events(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SubmissionEvent>)> {
    use crate::schema::md_submission_completed_event::dsl::id;
    let count = submission_event_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = submission_event_query(search.as_deref())
        .order(id.desc())
        .select(SubmissionEvent::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_submission_event(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<SubmissionEvent> {
    md_submission_completed_event::table
        .find(rid)
        .select(SubmissionEvent::as_select())
        .first(conn)
}

pub fn insert_submission_event(
    conn: &mut PgConnection,
    new: NewSubmissionEvent,
) -> QueryResult<SubmissionEvent> {
    diesel::insert_into(md_submission_completed_event::table)
        .values(&new)
        .returning(SubmissionEvent::as_returning())
        .get_result(conn)
}

pub fn update_submission_event(
    conn: &mut PgConnection,
    rid: i64,
    cs: SubmissionEventUpdate,
) -> QueryResult<SubmissionEvent> {
    diesel::update(md_submission_completed_event::table.find(rid))
        .set(&cs)
        .returning(SubmissionEvent::as_returning())
        .get_result(conn)
}

pub fn delete_submission_event(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<usize> {
    diesel::delete(md_submission_completed_event::table.find(rid)).execute(conn)
}

// ── md_ticket ─────────────────────────────────────────────────────────────────

fn ticket_query(
    search: Option<&str>,
    creator_id: Option<i64>,
) -> md_ticket::BoxedQuery<'static, Pg> {
    use crate::schema::md_ticket::dsl::*;
    let mut q = md_ticket.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(token.ilike(p.clone()).or(full_token.ilike(p)));
    }
    if let Some(c) = creator_id {
        q = q.filter(created_by_id.eq(c));
    }
    q
}

pub fn list_tickets(
    conn: &mut PgConnection,
    search: Option<String>,
    creator_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Ticket>)> {
    use crate::schema::md_ticket::dsl::id;
    let count = ticket_query(search.as_deref(), creator_id)
        .select(count_star())
        .first(conn)?;
    let mut q = ticket_query(search.as_deref(), creator_id)
        .order(id.desc())
        .select(Ticket::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_ticket(conn: &mut PgConnection, rid: i64) -> QueryResult<Ticket> {
    md_ticket::table
        .find(rid)
        .select(Ticket::as_select())
        .first(conn)
}

pub fn insert_ticket(conn: &mut PgConnection, new: NewTicket) -> QueryResult<Ticket> {
    diesel::insert_into(md_ticket::table)
        .values(&new)
        .returning(Ticket::as_returning())
        .get_result(conn)
}

pub fn update_ticket(
    conn: &mut PgConnection,
    rid: i64,
    cs: TicketUpdate,
) -> QueryResult<Ticket> {
    diesel::update(md_ticket::table.find(rid))
        .set(&cs)
        .returning(Ticket::as_returning())
        .get_result(conn)
}

pub fn delete_ticket(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_ticket::table.find(rid)).execute(conn)
}

// ── md_uniprot ────────────────────────────────────────────────────────────────

fn uniprot_query(search: Option<&str>) -> md_uniprot::BoxedQuery<'static, Pg> {
    use crate::schema::md_uniprot::dsl::*;
    let mut q = md_uniprot.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(uniprot_id.ilike(p)));
    }
    q
}

pub fn list_uniprots(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Uniprot>)> {
    use crate::schema::md_uniprot::dsl::id;
    let count = uniprot_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = uniprot_query(search.as_deref())
        .order(id.desc())
        .select(Uniprot::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_uniprot(conn: &mut PgConnection, rid: i64) -> QueryResult<Uniprot> {
    md_uniprot::table
        .find(rid)
        .select(Uniprot::as_select())
        .first(conn)
}

pub fn insert_uniprot(
    conn: &mut PgConnection,
    new: NewUniprot,
) -> QueryResult<Uniprot> {
    diesel::insert_into(md_uniprot::table)
        .values(&new)
        .returning(Uniprot::as_returning())
        .get_result(conn)
}

/// Insert the accession, or refresh the existing row, in one statement.
///
/// This exists because find-then-insert races. md_uniprot rows are shared
/// across simulations, and mdr-process processes landing directories in
/// parallel, so two threads importing simulations that cite the *same* new
/// accession both find nothing and both insert. Ticket 2175 died that way on
/// 2026-08-09: landings _7 and _15 both carried P35557, _7 inserted it, and
/// _15 failed the whole directory with "duplicate key value violates unique
/// constraint md_repo_app_uniprot_uniprot_id_key".
///
/// It is a first-encounter race -- once the row exists every later thread
/// takes the update path -- which is exactly why it is easy to miss and why a
/// batch of one submitter's variants of a single protein is the case that
/// triggers it.
///
/// DO UPDATE rather than DO NOTHING: DO NOTHING returns no row on conflict, so
/// get_result would fail with NotFound precisely when we lost the race. The
/// SET list mirrors what the old update branch wrote, so behaviour for an
/// existing row is unchanged.
pub fn upsert_uniprot(
    conn: &mut PgConnection,
    new: NewUniprot,
) -> QueryResult<Uniprot> {
    use diesel::upsert::excluded;

    diesel::insert_into(md_uniprot::table)
        .values(&new)
        .on_conflict(md_uniprot::uniprot_id)
        .do_update()
        .set((
            md_uniprot::name.eq(excluded(md_uniprot::name)),
            md_uniprot::amino_length.eq(excluded(md_uniprot::amino_length)),
            md_uniprot::sequence.eq(excluded(md_uniprot::sequence)),
        ))
        .returning(Uniprot::as_returning())
        .get_result(conn)
}

pub fn update_uniprot(
    conn: &mut PgConnection,
    rid: i64,
    cs: UniprotUpdate,
) -> QueryResult<Uniprot> {
    diesel::update(md_uniprot::table.find(rid))
        .set(&cs)
        .returning(Uniprot::as_returning())
        .get_result(conn)
}

pub fn delete_uniprot(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_uniprot::table.find(rid)).execute(conn)
}

// ── md_upload_instance ────────────────────────────────────────────────────────

fn upload_instance_query(
    sim_id: Option<i64>,
    uid: Option<i64>,
    tkt_id: Option<i64>,
) -> md_upload_instance::BoxedQuery<'static, Pg> {
    use crate::schema::md_upload_instance::dsl::*;
    let mut q = md_upload_instance.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(u) = uid {
        q = q.filter(user_id.eq(u));
    }
    if let Some(t) = tkt_id {
        q = q.filter(ticket_id.eq(t));
    }
    q
}

pub fn list_upload_instances(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    uid: Option<i64>,
    tkt_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<UploadInstance>)> {
    use crate::schema::md_upload_instance::dsl::id;
    let count = upload_instance_query(sim_id, uid, tkt_id)
        .select(count_star())
        .first(conn)?;
    let mut q = upload_instance_query(sim_id, uid, tkt_id)
        .order(id.desc())
        .select(UploadInstance::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_upload_instance(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<UploadInstance> {
    md_upload_instance::table
        .find(rid)
        .select(UploadInstance::as_select())
        .first(conn)
}

pub fn insert_upload_instance(
    conn: &mut PgConnection,
    new: NewUploadInstance,
) -> QueryResult<UploadInstance> {
    diesel::insert_into(md_upload_instance::table)
        .values(&new)
        .returning(UploadInstance::as_returning())
        .get_result(conn)
}

pub fn update_upload_instance(
    conn: &mut PgConnection,
    rid: i64,
    cs: UploadInstanceUpdate,
) -> QueryResult<UploadInstance> {
    diesel::update(md_upload_instance::table.find(rid))
        .set(&cs)
        .returning(UploadInstance::as_returning())
        .get_result(conn)
}

pub fn delete_upload_instance(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_upload_instance::table.find(rid)).execute(conn)
}

// ── md_upload_instance_message ────────────────────────────────────────────────

fn upload_message_query(
    search: Option<&str>,
    upload_id: Option<i64>,
    errors_only: bool,
    warnings_only: bool,
) -> md_upload_instance_message::BoxedQuery<'static, Pg> {
    use crate::schema::md_upload_instance_message::dsl::*;
    let mut q = md_upload_instance_message.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(message.ilike(p));
    }
    if let Some(u) = upload_id {
        q = q.filter(simulation_upload_id.eq(u));
    }
    if errors_only {
        q = q.filter(is_error.eq(true));
    }
    if warnings_only {
        q = q.filter(is_warning.eq(true));
    }
    q
}

pub fn list_upload_messages(
    conn: &mut PgConnection,
    search: Option<String>,
    upload_id: Option<i64>,
    errors_only: bool,
    warnings_only: bool,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<UploadMessage>)> {
    use crate::schema::md_upload_instance_message::dsl::id;
    let count =
        upload_message_query(search.as_deref(), upload_id, errors_only, warnings_only)
            .select(count_star())
            .first(conn)?;
    let mut q =
        upload_message_query(search.as_deref(), upload_id, errors_only, warnings_only)
            .order(id.desc())
            .select(UploadMessage::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_upload_message(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<UploadMessage> {
    md_upload_instance_message::table
        .find(rid)
        .select(UploadMessage::as_select())
        .first(conn)
}

pub fn insert_upload_message(
    conn: &mut PgConnection,
    new: NewUploadMessage,
) -> QueryResult<UploadMessage> {
    diesel::insert_into(md_upload_instance_message::table)
        .values(&new)
        .returning(UploadMessage::as_returning())
        .get_result(conn)
}

pub fn update_upload_message(
    conn: &mut PgConnection,
    rid: i64,
    cs: UploadMessageUpdate,
) -> QueryResult<UploadMessage> {
    diesel::update(md_upload_instance_message::table.find(rid))
        .set(&cs)
        .returning(UploadMessage::as_returning())
        .get_result(conn)
}

pub fn delete_upload_message(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_upload_instance_message::table.find(rid)).execute(conn)
}

// ── md_uploaded_file ──────────────────────────────────────────────────────────

fn uploaded_file_query(
    search: Option<&str>,
    sim_id: Option<i64>,
) -> md_uploaded_file::BoxedQuery<'static, Pg> {
    use crate::schema::md_uploaded_file::dsl::*;
    let mut q = md_uploaded_file.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(filename.ilike(p.clone()).or(file_type.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    q
}

pub fn list_uploaded_files(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<UploadedFile>)> {
    use crate::schema::md_uploaded_file::dsl::id;
    let count = uploaded_file_query(search.as_deref(), sim_id)
        .select(count_star())
        .first(conn)?;
    let mut q = uploaded_file_query(search.as_deref(), sim_id)
        .order(id.desc())
        .select(UploadedFile::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_uploaded_file(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<UploadedFile> {
    md_uploaded_file::table
        .find(rid)
        .select(UploadedFile::as_select())
        .first(conn)
}

pub fn insert_uploaded_file(
    conn: &mut PgConnection,
    new: NewUploadedFile,
) -> QueryResult<UploadedFile> {
    diesel::insert_into(md_uploaded_file::table)
        .values(&new)
        .returning(UploadedFile::as_returning())
        .get_result(conn)
}

pub fn update_uploaded_file(
    conn: &mut PgConnection,
    rid: i64,
    cs: UploadedFileUpdate,
) -> QueryResult<UploadedFile> {
    diesel::update(md_uploaded_file::table.find(rid))
        .set(&cs)
        .returning(UploadedFile::as_returning())
        .get_result(conn)
}

pub fn delete_uploaded_file(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_uploaded_file::table.find(rid)).execute(conn)
}

// ── md_user ───────────────────────────────────────────────────────────────────

fn user_query(
    search: Option<&str>,
    active_only: bool,
) -> md_user::BoxedQuery<'static, Pg> {
    use crate::schema::md_user::dsl::*;
    let mut q = md_user.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(username.ilike(p.clone()).or(email.ilike(p)));
    }
    if active_only {
        q = q.filter(is_active.eq(true));
    }
    q
}

pub fn list_users(
    conn: &mut PgConnection,
    search: Option<String>,
    active_only: bool,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<User>)> {
    use crate::schema::md_user::dsl::id;
    let count = user_query(search.as_deref(), active_only)
        .select(count_star())
        .first(conn)?;
    let mut q = user_query(search.as_deref(), active_only)
        .order(id.desc())
        .select(User::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_user(conn: &mut PgConnection, rid: i64) -> QueryResult<User> {
    md_user::table
        .find(rid)
        .select(User::as_select())
        .first(conn)
}

pub fn insert_user(conn: &mut PgConnection, new: NewUser) -> QueryResult<User> {
    diesel::insert_into(md_user::table)
        .values(&new)
        .returning(User::as_returning())
        .get_result(conn)
}

pub fn update_user(
    conn: &mut PgConnection,
    rid: i64,
    cs: UserUpdate,
) -> QueryResult<User> {
    diesel::update(md_user::table.find(rid))
        .set(&cs)
        .returning(User::as_returning())
        .get_result(conn)
}

pub fn delete_user(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_user::table.find(rid)).execute(conn)
}

// ── socialaccount_socialaccount ────────────────────────────────────────────────

fn social_account_query(
    search: Option<&str>,
    user_id_filter: Option<i64>,
) -> socialaccount_socialaccount::BoxedQuery<'static, Pg> {
    use crate::schema::socialaccount_socialaccount::dsl::*;
    let mut q = socialaccount_socialaccount.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(provider.ilike(p.clone()).or(uid.ilike(p)));
    }
    if let Some(u) = user_id_filter {
        q = q.filter(user_id.eq(u));
    }
    q
}

pub fn list_social_accounts(
    conn: &mut PgConnection,
    search: Option<String>,
    user_id_filter: Option<i64>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SocialAccount>)> {
    use crate::schema::socialaccount_socialaccount::dsl::id;
    let count = social_account_query(search.as_deref(), user_id_filter)
        .select(count_star())
        .first(conn)?;
    let mut q = social_account_query(search.as_deref(), user_id_filter)
        .order(id.desc())
        .select(SocialAccount::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_social_account(
    conn: &mut PgConnection,
    rid: i32,
) -> QueryResult<SocialAccount> {
    socialaccount_socialaccount::table
        .find(rid)
        .select(SocialAccount::as_select())
        .first(conn)
}

pub fn insert_social_account(
    conn: &mut PgConnection,
    new: NewSocialAccount,
) -> QueryResult<SocialAccount> {
    diesel::insert_into(socialaccount_socialaccount::table)
        .values(&new)
        .returning(SocialAccount::as_returning())
        .get_result(conn)
}

pub fn update_social_account(
    conn: &mut PgConnection,
    rid: i32,
    cs: SocialAccountUpdate,
) -> QueryResult<SocialAccount> {
    diesel::update(socialaccount_socialaccount::table.find(rid))
        .set(&cs)
        .returning(SocialAccount::as_returning())
        .get_result(conn)
}

pub fn delete_social_account(conn: &mut PgConnection, rid: i32) -> QueryResult<usize> {
    diesel::delete(socialaccount_socialaccount::table.find(rid)).execute(conn)
}

// ── import: natural-key finders ───────────────────────────────────────────────
//
// These back the DB-import port (replacing `import_preprocessed.py`). Each is a
// pure "find" — it returns `Ok(None)` when there is no match rather than
// inserting — so orchestration can decide find-or-create. They mirror the
// SELECTs the script does before each `create_*`.

/// User id for an ORCID, via the `orcid` social account. Mirrors the script's
/// `get_user`: join `socialaccount_socialaccount` (provider `orcid`) to `md_user`.
pub fn find_user_id_by_orcid(
    conn: &mut PgConnection,
    orcid_uid: &str,
) -> QueryResult<Option<i64>> {
    md_user::table
        .inner_join(
            socialaccount_socialaccount::table
                .on(socialaccount_socialaccount::user_id.eq(md_user::id)),
        )
        .filter(socialaccount_socialaccount::provider.eq("orcid"))
        .filter(socialaccount_socialaccount::uid.eq(orcid_uid))
        .select(md_user::id)
        .first::<i64>(conn)
        .optional()
}

/// User id for a username. Used to resolve fixed system accounts (e.g. the
/// admin user imports are attributed to) by a stable, self-documenting name
/// rather than a value like an ORCID that only means something by convention.
pub fn find_user_id_by_username(
    conn: &mut PgConnection,
    uname: &str,
) -> QueryResult<Option<i64>> {
    md_user::table
        .filter(md_user::username.eq(uname))
        .select(md_user::id)
        .first::<i64>(conn)
        .optional()
}

/// Software id by its natural key `(name, version)`. Mirrors `create_software`'s
/// lookup. NOTE: this diverges deliberately from the Python for a NULL version —
/// the script queries `version = %s`, which never matches when the version is
/// None (so it always inserts a fresh row); here a None version matches an
/// existing `version IS NULL` row, which is the correct dedup behavior.
pub fn find_software_id_by_name_version(
    conn: &mut PgConnection,
    sw_name: &str,
    sw_version: Option<&str>,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_software::dsl::*;
    let mut q = md_software.filter(name.eq(sw_name)).into_boxed();
    q = match sw_version {
        Some(v) => q.filter(version.eq(v)),
        None => q.filter(version.is_null()),
    };
    q.select(id).first::<i64>(conn).optional()
}

/// Pub id by DOI — the script's preferred dedup key when a DOI is present.
pub fn find_pub_id_by_doi(
    conn: &mut PgConnection,
    pub_doi: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_pub::dsl::*;
    md_pub
        .filter(doi.eq(pub_doi))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Pub id by metadata — the script's dedup fallback when there is no DOI.
pub fn find_pub_id_by_metadata(
    conn: &mut PgConnection,
    pub_title: &str,
    pub_authors: &str,
    pub_journal: &str,
    pub_volume: i32,
    pub_year: i32,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_pub::dsl::*;
    md_pub
        .filter(title.eq(pub_title))
        .filter(authors.eq(pub_authors))
        .filter(journal.eq(pub_journal))
        .filter(volume.eq(pub_volume))
        .filter(year.eq(pub_year))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Existing `md_simulation_pub` link id for `(simulation_id, pub_id)`, so the
/// importer doesn't create a duplicate link (mirrors `create_paper`'s guard).
pub fn find_simulation_pub_id(
    conn: &mut PgConnection,
    sim_id: i64,
    pid: i64,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_simulation_pub::dsl::*;
    md_simulation_pub
        .filter(simulation_id.eq(sim_id))
        .filter(pub_id.eq(pid))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Simulation id by `(alias, created_by_id)` — the first `get_simulation` upsert
/// probe. A `None` creator matches a row whose `created_by_id IS NULL` (correct
/// per-user alias semantics; the Python's `created_by_id = NULL` never matched).
pub fn find_simulation_id_by_alias(
    conn: &mut PgConnection,
    sim_alias: &str,
    created_by: Option<i64>,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_simulation::dsl::*;
    let mut q = md_simulation.filter(alias.eq(sim_alias)).into_boxed();
    q = match created_by {
        Some(u) => q.filter(created_by_id.eq(u)),
        None => q.filter(created_by_id.is_null()),
    };
    q.select(id).first::<i64>(conn).optional()
}

/// Simulation id by its `unique_file_hash_string` — `get_simulation`'s second
/// probe, used to recognise a re-upload of the same files.
pub fn find_simulation_id_by_hash(
    conn: &mut PgConnection,
    hash: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_simulation::dsl::*;
    md_simulation
        .filter(unique_file_hash_string.eq(hash))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Which column identifies a contributor within a simulation. The script tries
/// ORCID, then email, then name — the caller picks, so that precedence stays
/// visible at the call site.
pub enum ContributionKey<'a> {
    Orcid(&'a str),
    Email(&'a str),
    Name(&'a str),
}

/// Contribution id for a simulation, by whichever natural key the caller has.
pub fn find_contribution_id(
    conn: &mut PgConnection,
    sim_id: i64,
    key: ContributionKey<'_>,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_contribution::dsl::*;
    let q = md_contribution
        .filter(simulation_id.eq(sim_id))
        .into_boxed();
    let q = match key {
        ContributionKey::Orcid(v) => q.filter(orcid.eq(v)),
        ContributionKey::Email(v) => q.filter(email.eq(v)),
        ContributionKey::Name(v) => q.filter(name.eq(v)),
    };
    q.select(id).first::<i64>(conn).optional()
}

/// Processed-file id by `(simulation_id, filename)`.
pub fn find_processed_file_id(
    conn: &mut PgConnection,
    sim_id: i64,
    name: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_processed_file::dsl::*;
    md_processed_file
        .filter(simulation_id.eq(sim_id))
        .filter(filename.eq(name))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Uploaded-file id by `(simulation_id, filename)`.
pub fn find_uploaded_file_id(
    conn: &mut PgConnection,
    sim_id: i64,
    name: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_uploaded_file::dsl::*;
    md_uploaded_file
        .filter(simulation_id.eq(sim_id))
        .filter(filename.eq(name))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Replicate id by `(simulation_id, trajectory_file_name)`.
pub fn find_replicate_id(
    conn: &mut PgConnection,
    sim_id: i64,
    trajectory: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_replicate::dsl::*;
    md_replicate
        .filter(simulation_id.eq(sim_id))
        .filter(trajectory_file_name.eq(trajectory))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Ligand id by `(simulation_id, name)`.
pub fn find_ligand_id(
    conn: &mut PgConnection,
    sim_id: i64,
    ligand_name: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_ligand::dsl::*;
    md_ligand
        .filter(simulation_id.eq(sim_id))
        .filter(name.eq(ligand_name))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Solute id by `(simulation_id, name)`.
pub fn find_solute_id(
    conn: &mut PgConnection,
    sim_id: i64,
    solute_name: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_solute::dsl::*;
    md_solute
        .filter(simulation_id.eq(sim_id))
        .filter(name.eq(solute_name))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// External-link id by `(simulation_id, url)`.
pub fn find_external_link_id(
    conn: &mut PgConnection,
    sim_id: i64,
    link_url: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_external_link::dsl::*;
    md_external_link
        .filter(simulation_id.eq(sim_id))
        .filter(url.eq(link_url))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Uniprot row id for a UniProt accession (`md_uniprot.uniprot_id`, the string
/// accession — not the primary key).
pub fn find_uniprot_id_by_accession(
    conn: &mut PgConnection,
    accession: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_uniprot::dsl::*;
    md_uniprot
        .filter(uniprot_id.eq(accession))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// Existing `md_simulation_uniprot` link id for `(simulation_id, uniprot_id)`.
pub fn find_simulation_uniprot_id(
    conn: &mut PgConnection,
    sim_id: i64,
    uniprot_pk: i64,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_simulation_uniprot::dsl::*;
    md_simulation_uniprot
        .filter(simulation_id.eq(sim_id))
        .filter(uniprot_id.eq(uniprot_pk))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

/// PDB row id for a PDB code (`md_pdb.pdb_id`, the string code — not the primary
/// key). The script lowercases the code before looking it up.
pub fn find_pdb_id_by_code(
    conn: &mut PgConnection,
    code: &str,
) -> QueryResult<Option<i64>> {
    use crate::schema::md_pdb::dsl::*;
    md_pdb
        .filter(pdb_id.eq(code))
        .select(id)
        .first::<i64>(conn)
        .optional()
}

// ── import: reprocess delete-cascade ──────────────────────────────────────────
//
// The DB has no ON DELETE CASCADE from the frontend-download link tables to the
// file tables, so — like the script — we delete the link rows first, then the
// files. Returns the number of *file* rows deleted.

/// Delete a simulation's processed files and their frontend-download links.
/// Used on the `--simulation-id` reprocess path.
pub fn delete_processed_files_for_simulation(
    conn: &mut PgConnection,
    sim_id: i64,
) -> QueryResult<usize> {
    use crate::schema::md_frontend_download_instance_processed_files::dsl as link;
    use crate::schema::md_processed_file::dsl as pf;

    let file_ids = pf::md_processed_file
        .filter(pf::simulation_id.eq(sim_id))
        .select(pf::id);
    diesel::delete(
        link::md_frontend_download_instance_processed_files
            .filter(link::simulationprocessedfile_id.eq_any(file_ids)),
    )
    .execute(conn)?;

    diesel::delete(pf::md_processed_file.filter(pf::simulation_id.eq(sim_id)))
        .execute(conn)
}

/// Delete a simulation's uploaded (original) files and their frontend-download
/// links. Used on the `--replace-original-files` reprocess path.
pub fn delete_uploaded_files_for_simulation(
    conn: &mut PgConnection,
    sim_id: i64,
) -> QueryResult<usize> {
    use crate::schema::md_frontend_download_instance_uploaded_files::dsl as link;
    use crate::schema::md_uploaded_file::dsl as uf;

    let file_ids = uf::md_uploaded_file
        .filter(uf::simulation_id.eq(sim_id))
        .select(uf::id);
    diesel::delete(
        link::md_frontend_download_instance_uploaded_files
            .filter(link::simulationuploadedfile_id.eq_any(file_ids)),
    )
    .execute(conn)?;

    diesel::delete(uf::md_uploaded_file.filter(uf::simulation_id.eq(sim_id)))
        .execute(conn)
}

/// Delete a simulation's `md_replicate` rows. Used on the reprocess path.
///
/// `upsert_replicate` is find-or-insert, so without this a reprocess whose
/// metadata names a different trajectory leaves the old row in place — and
/// `mdr-export` builds `trajectory_file_names` from this table rather than
/// from `is_primary`, so the stale name is published, not merely stored.
///
/// Nothing links to `md_replicate`, so there is no link table to clear first.
pub fn delete_replicates_for_simulation(
    conn: &mut PgConnection,
    sim_id: i64,
) -> QueryResult<usize> {
    use crate::schema::md_replicate::dsl as r;

    diesel::delete(r::md_replicate.filter(r::simulation_id.eq(sim_id))).execute(conn)
}

/// Delete a simulation's `md_simulation_uniprot` links. Used on the reprocess
/// path.
///
/// This clears the join rows only; the `md_uniprot` accessions they point at
/// are shared across simulations and are left alone. `upsert_uniprot` is
/// find-or-insert like the replicate case, so a correction made between two
/// uploads otherwise leaves both the corrected and the mistaken link behind —
/// simulation 84214 carried a stale `P01892` that way.
pub fn delete_uniprots_for_simulation(
    conn: &mut PgConnection,
    sim_id: i64,
) -> QueryResult<usize> {
    use crate::schema::md_simulation_uniprot::dsl as su;

    diesel::delete(su::md_simulation_uniprot.filter(su::simulation_id.eq(sim_id)))
        .execute(conn)
}
