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
        q = q.filter(name.ilike(p.clone()).or(smiles_string.ilike(p)));
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

// ── md_replicate_group ────────────────────────────────────────────────────────

fn replicate_group_query(
    search: Option<&str>,
) -> md_replicate_group::BoxedQuery<'static, Pg> {
    use crate::schema::md_replicate_group::dsl::*;
    let mut q = md_replicate_group.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(replicate_key.ilike(p.clone()).or(sample_mdrepo_id.ilike(p)));
    }
    q
}

pub fn list_replicate_groups(
    conn: &mut PgConnection,
    search: Option<String>,
    all: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<ReplicateGroup>)> {
    use crate::schema::md_replicate_group::dsl::id;
    let count = replicate_group_query(search.as_deref())
        .select(count_star())
        .first(conn)?;
    let mut q = replicate_group_query(search.as_deref())
        .order(id.desc())
        .select(ReplicateGroup::as_select());
    if !all {
        q = q.limit(limit(lim)).offset(offset.unwrap_or(0));
    }
    let results = q.load(conn)?;
    Ok((count, results))
}

pub fn get_replicate_group(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<ReplicateGroup> {
    md_replicate_group::table
        .find(rid)
        .select(ReplicateGroup::as_select())
        .first(conn)
}

pub fn insert_replicate_group(
    conn: &mut PgConnection,
    new: NewReplicateGroup,
) -> QueryResult<ReplicateGroup> {
    diesel::insert_into(md_replicate_group::table)
        .values(&new)
        .returning(ReplicateGroup::as_returning())
        .get_result(conn)
}

pub fn update_replicate_group(
    conn: &mut PgConnection,
    rid: i64,
    cs: ReplicateGroupUpdate,
) -> QueryResult<ReplicateGroup> {
    diesel::update(md_replicate_group::table.find(rid))
        .set(&cs)
        .returning(ReplicateGroup::as_returning())
        .get_result(conn)
}

pub fn delete_replicate_group(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_replicate_group::table.find(rid)).execute(conn)
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
