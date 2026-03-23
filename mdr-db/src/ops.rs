use diesel::dsl::count_star;
use diesel::prelude::*;

use crate::models::*;
use crate::schema::*;

// ── helpers ───────────────────────────────────────────────────────────────────

fn limit(n: Option<i64>) -> i64 {
    n.unwrap_or(200).min(1000)
}

// ── md_contribution ───────────────────────────────────────────────────────────

pub fn list_contributions(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Contribution>)> {
    use crate::schema::md_contribution::dsl::*;

    let mut cq = md_contribution.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(name.ilike(p.clone()).or(email.ilike(p)));
    }
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_contribution.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(email.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Contribution::as_select())
        .load(conn)?;

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

pub fn list_external_links(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<ExternalLink>)> {
    use crate::schema::md_external_link::dsl::*;

    let mut cq = md_external_link.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(url.ilike(p.clone()).or(label.ilike(p)));
    }
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_external_link.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(url.ilike(p.clone()).or(label.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(ExternalLink::as_select())
        .load(conn)?;

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
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<FeatureSwitch>)> {
    use crate::schema::md_feature_switch::dsl::*;

    let count: i64 = md_feature_switch.count().get_result(conn)?;
    let results = md_feature_switch
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(FeatureSwitch::as_select())
        .load(conn)?;

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

pub fn list_download_instances(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    uid: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<DownloadInstance>)> {
    use crate::schema::md_frontend_download_instance::dsl::*;

    let mut cq = md_frontend_download_instance.into_boxed();
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    if let Some(u) = uid {
        cq = cq.filter(user_id.eq(u));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_frontend_download_instance.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(u) = uid {
        q = q.filter(user_id.eq(u));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(DownloadInstance::as_select())
        .load(conn)?;

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

pub fn list_download_processed_files(
    conn: &mut PgConnection,
    di_id: Option<i64>,
    pf_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<DownloadProcessedFile>)> {
    use crate::schema::md_frontend_download_instance_processed_files::dsl::*;

    let mut cq = md_frontend_download_instance_processed_files.into_boxed();
    if let Some(v) = di_id {
        cq = cq.filter(frontenddownloadinstance_id.eq(v));
    }
    if let Some(v) = pf_id {
        cq = cq.filter(simulationprocessedfile_id.eq(v));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_frontend_download_instance_processed_files.into_boxed();
    if let Some(v) = di_id {
        q = q.filter(frontenddownloadinstance_id.eq(v));
    }
    if let Some(v) = pf_id {
        q = q.filter(simulationprocessedfile_id.eq(v));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(DownloadProcessedFile::as_select())
        .load(conn)?;

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

pub fn list_download_uploaded_files(
    conn: &mut PgConnection,
    di_id: Option<i64>,
    uf_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<DownloadUploadedFile>)> {
    use crate::schema::md_frontend_download_instance_uploaded_files::dsl::*;

    let mut cq = md_frontend_download_instance_uploaded_files.into_boxed();
    if let Some(v) = di_id {
        cq = cq.filter(frontenddownloadinstance_id.eq(v));
    }
    if let Some(v) = uf_id {
        cq = cq.filter(simulationuploadedfile_id.eq(v));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_frontend_download_instance_uploaded_files.into_boxed();
    if let Some(v) = di_id {
        q = q.filter(frontenddownloadinstance_id.eq(v));
    }
    if let Some(v) = uf_id {
        q = q.filter(simulationuploadedfile_id.eq(v));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(DownloadUploadedFile::as_select())
        .load(conn)?;

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

pub fn list_ligands(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Ligand>)> {
    use crate::schema::md_ligand::dsl::*;

    let mut cq = md_ligand.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(name.ilike(p.clone()).or(smiles_string.ilike(p)));
    }
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_ligand.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(smiles_string.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Ligand::as_select())
        .load(conn)?;

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

pub fn list_pdbs(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Pdb>)> {
    use crate::schema::md_pdb::dsl::*;

    let mut cq = md_pdb.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(
            pdb_id
                .ilike(p.clone())
                .or(title.ilike(p.clone()))
                .or(classification.ilike(p)),
        );
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

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
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Pdb::as_select())
        .load(conn)?;

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

pub fn list_processed_files(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<ProcessedFile>)> {
    use crate::schema::md_processed_file::dsl::*;

    let mut cq = md_processed_file.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(filename.ilike(p.clone()).or(file_type.ilike(p)));
    }
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_processed_file.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(filename.ilike(p.clone()).or(file_type.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(ProcessedFile::as_select())
        .load(conn)?;

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

// ── md_pub ────────────────────────────────────────────────────────────────────

pub fn list_pubs(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Pub>)> {
    use crate::schema::md_pub::dsl::*;

    let mut cq = md_pub.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(
            title
                .ilike(p.clone())
                .or(authors.ilike(p.clone()))
                .or(journal.ilike(p)),
        );
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

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
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Pub::as_select())
        .load(conn)?;

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

pub fn list_simulations(
    conn: &mut PgConnection,
    search: Option<String>,
    public_only: bool,
    active: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Simulation>)> {
    use crate::schema::md_simulation::dsl::*;

    let mut cq = md_simulation.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(description.ilike(p.clone()).or(short_description.ilike(p)));
    }
    if public_only {
        cq = cq.filter(is_public.eq(true));
    }
    if active {
        cq = cq.filter(is_deprecated.eq(false));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

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
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Simulation::as_select())
        .load(conn)?;

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

pub fn list_simulation_pubs(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    pid: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SimulationPub>)> {
    use crate::schema::md_simulation_pub::dsl::*;

    let mut cq = md_simulation_pub.into_boxed();
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    if let Some(p) = pid {
        cq = cq.filter(pub_id.eq(p));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_simulation_pub.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(p) = pid {
        q = q.filter(pub_id.eq(p));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SimulationPub::as_select())
        .load(conn)?;

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

// ── md_simulation_replicate_group ─────────────────────────────────────────────

pub fn list_replicate_groups(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<ReplicateGroup>)> {
    use crate::schema::md_simulation_replicate_group::dsl::*;

    let mut cq = md_simulation_replicate_group.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(psf_hash.ilike(p));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_simulation_replicate_group.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(psf_hash.ilike(p));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(ReplicateGroup::as_select())
        .load(conn)?;

    Ok((count, results))
}

pub fn get_replicate_group(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<ReplicateGroup> {
    md_simulation_replicate_group::table
        .find(rid)
        .select(ReplicateGroup::as_select())
        .first(conn)
}

pub fn insert_replicate_group(
    conn: &mut PgConnection,
    new: NewReplicateGroup,
) -> QueryResult<ReplicateGroup> {
    diesel::insert_into(md_simulation_replicate_group::table)
        .values(&new)
        .returning(ReplicateGroup::as_returning())
        .get_result(conn)
}

pub fn update_replicate_group(
    conn: &mut PgConnection,
    rid: i64,
    cs: ReplicateGroupUpdate,
) -> QueryResult<ReplicateGroup> {
    diesel::update(md_simulation_replicate_group::table.find(rid))
        .set(&cs)
        .returning(ReplicateGroup::as_returning())
        .get_result(conn)
}

pub fn delete_replicate_group(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_simulation_replicate_group::table.find(rid)).execute(conn)
}

// ── md_simulation_uniprot ─────────────────────────────────────────────────────

pub fn list_simulation_uniprots(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    upid: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SimulationUniprot>)> {
    use crate::schema::md_simulation_uniprot::dsl::*;

    let mut cq = md_simulation_uniprot.into_boxed();
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    if let Some(u) = upid {
        cq = cq.filter(uniprot_id.eq(u));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_simulation_uniprot.into_boxed();
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    if let Some(u) = upid {
        q = q.filter(uniprot_id.eq(u));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SimulationUniprot::as_select())
        .load(conn)?;

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

pub fn list_software(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Software>)> {
    use crate::schema::md_software::dsl::*;

    let mut cq = md_software.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(name.ilike(p));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_software.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Software::as_select())
        .load(conn)?;

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

// ── md_solvent ────────────────────────────────────────────────────────────────

pub fn list_solvents(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Solvent>)> {
    use crate::schema::md_solvent::dsl::*;

    let mut cq = md_solvent.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(name.ilike(p));
    }
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_solvent.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Solvent::as_select())
        .load(conn)?;

    Ok((count, results))
}

pub fn get_solvent(conn: &mut PgConnection, rid: i64) -> QueryResult<Solvent> {
    md_solvent::table
        .find(rid)
        .select(Solvent::as_select())
        .first(conn)
}

pub fn insert_solvent(
    conn: &mut PgConnection,
    new: NewSolvent,
) -> QueryResult<Solvent> {
    diesel::insert_into(md_solvent::table)
        .values(&new)
        .returning(Solvent::as_returning())
        .get_result(conn)
}

pub fn update_solvent(
    conn: &mut PgConnection,
    rid: i64,
    cs: SolventUpdate,
) -> QueryResult<Solvent> {
    diesel::update(md_solvent::table.find(rid))
        .set(&cs)
        .returning(Solvent::as_returning())
        .get_result(conn)
}

pub fn delete_solvent(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(md_solvent::table.find(rid)).execute(conn)
}

// ── md_submission_completed_event ─────────────────────────────────────────────

pub fn list_submission_events(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SubmissionEvent>)> {
    use crate::schema::md_submission_completed_event::dsl::*;

    let mut cq = md_submission_completed_event.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(path.ilike(p));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_submission_completed_event.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(path.ilike(p));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SubmissionEvent::as_select())
        .load(conn)?;

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

pub fn list_tickets(
    conn: &mut PgConnection,
    search: Option<String>,
    creator_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Ticket>)> {
    use crate::schema::md_ticket::dsl::*;

    let mut cq = md_ticket.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(token.ilike(p.clone()).or(full_token.ilike(p)));
    }
    if let Some(c) = creator_id {
        cq = cq.filter(created_by_id.eq(c));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_ticket.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(token.ilike(p.clone()).or(full_token.ilike(p)));
    }
    if let Some(c) = creator_id {
        q = q.filter(created_by_id.eq(c));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Ticket::as_select())
        .load(conn)?;

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

pub fn list_uniprots(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<Uniprot>)> {
    use crate::schema::md_uniprot::dsl::*;

    let mut cq = md_uniprot.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(name.ilike(p.clone()).or(uniprot_id.ilike(p)));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_uniprot.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(uniprot_id.ilike(p)));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(Uniprot::as_select())
        .load(conn)?;

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

pub fn list_upload_instances(
    conn: &mut PgConnection,
    sim_id: Option<i64>,
    uid: Option<i64>,
    tkt_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<UploadInstance>)> {
    use crate::schema::md_upload_instance::dsl::*;

    let mut cq = md_upload_instance.into_boxed();
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    if let Some(u) = uid {
        cq = cq.filter(user_id.eq(u));
    }
    if let Some(t) = tkt_id {
        cq = cq.filter(ticket_id.eq(t));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

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
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(UploadInstance::as_select())
        .load(conn)?;

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

pub fn list_upload_messages(
    conn: &mut PgConnection,
    search: Option<String>,
    upload_id: Option<i64>,
    errors_only: bool,
    warnings_only: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<UploadMessage>)> {
    use crate::schema::md_upload_instance_message::dsl::*;

    let mut cq = md_upload_instance_message.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(message.ilike(p));
    }
    if let Some(u) = upload_id {
        cq = cq.filter(simulation_upload_id.eq(u));
    }
    if errors_only {
        cq = cq.filter(is_error.eq(true));
    }
    if warnings_only {
        cq = cq.filter(is_warning.eq(true));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

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
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(UploadMessage::as_select())
        .load(conn)?;

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

pub fn list_uploaded_files(
    conn: &mut PgConnection,
    search: Option<String>,
    sim_id: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<UploadedFile>)> {
    use crate::schema::md_uploaded_file::dsl::*;

    let mut cq = md_uploaded_file.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(filename.ilike(p.clone()).or(file_type.ilike(p)));
    }
    if let Some(s) = sim_id {
        cq = cq.filter(simulation_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_uploaded_file.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(filename.ilike(p.clone()).or(file_type.ilike(p)));
    }
    if let Some(s) = sim_id {
        q = q.filter(simulation_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(UploadedFile::as_select())
        .load(conn)?;

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

pub fn list_users(
    conn: &mut PgConnection,
    search: Option<String>,
    active_only: bool,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<User>)> {
    use crate::schema::md_user::dsl::*;

    let mut cq = md_user.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(username.ilike(p.clone()).or(email.ilike(p)));
    }
    if active_only {
        cq = cq.filter(is_active.eq(true));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = md_user.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(username.ilike(p.clone()).or(email.ilike(p)));
    }
    if active_only {
        q = q.filter(is_active.eq(true));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(User::as_select())
        .load(conn)?;

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

pub fn list_social_accounts(
    conn: &mut PgConnection,
    search: Option<String>,
    user_id_filter: Option<i64>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SocialAccount>)> {
    use crate::schema::socialaccount_socialaccount::dsl::*;

    let mut cq = socialaccount_socialaccount.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(provider.ilike(p.clone()).or(uid.ilike(p)));
    }
    if let Some(u) = user_id_filter {
        cq = cq.filter(user_id.eq(u));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = socialaccount_socialaccount.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(provider.ilike(p.clone()).or(uid.ilike(p)));
    }
    if let Some(u) = user_id_filter {
        q = q.filter(user_id.eq(u));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SocialAccount::as_select())
        .load(conn)?;

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

// ── socialaccount_socialapp ────────────────────────────────────────────────────

pub fn list_social_apps(
    conn: &mut PgConnection,
    search: Option<String>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SocialApp>)> {
    use crate::schema::socialaccount_socialapp::dsl::*;

    let mut cq = socialaccount_socialapp.into_boxed();
    if let Some(ref t) = search {
        let p = format!("%{t}%");
        cq = cq.filter(name.ilike(p.clone()).or(provider.ilike(p)));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = socialaccount_socialapp.into_boxed();
    if let Some(t) = search {
        let p = format!("%{t}%");
        q = q.filter(name.ilike(p.clone()).or(provider.ilike(p)));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SocialApp::as_select())
        .load(conn)?;

    Ok((count, results))
}

pub fn get_social_app(conn: &mut PgConnection, rid: i32) -> QueryResult<SocialApp> {
    socialaccount_socialapp::table
        .find(rid)
        .select(SocialApp::as_select())
        .first(conn)
}

pub fn insert_social_app(
    conn: &mut PgConnection,
    new: NewSocialApp,
) -> QueryResult<SocialApp> {
    diesel::insert_into(socialaccount_socialapp::table)
        .values(&new)
        .returning(SocialApp::as_returning())
        .get_result(conn)
}

pub fn update_social_app(
    conn: &mut PgConnection,
    rid: i32,
    cs: SocialAppUpdate,
) -> QueryResult<SocialApp> {
    diesel::update(socialaccount_socialapp::table.find(rid))
        .set(&cs)
        .returning(SocialApp::as_returning())
        .get_result(conn)
}

pub fn delete_social_app(conn: &mut PgConnection, rid: i32) -> QueryResult<usize> {
    diesel::delete(socialaccount_socialapp::table.find(rid)).execute(conn)
}

// ── socialaccount_socialapp_sites ──────────────────────────────────────────────

pub fn list_social_app_sites(
    conn: &mut PgConnection,
    socialapp_id_filter: Option<i32>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SocialAppSite>)> {
    use crate::schema::socialaccount_socialapp_sites::dsl::*;

    let mut cq = socialaccount_socialapp_sites.into_boxed();
    if let Some(s) = socialapp_id_filter {
        cq = cq.filter(socialapp_id.eq(s));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = socialaccount_socialapp_sites.into_boxed();
    if let Some(s) = socialapp_id_filter {
        q = q.filter(socialapp_id.eq(s));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SocialAppSite::as_select())
        .load(conn)?;

    Ok((count, results))
}

pub fn get_social_app_site(
    conn: &mut PgConnection,
    rid: i64,
) -> QueryResult<SocialAppSite> {
    socialaccount_socialapp_sites::table
        .find(rid)
        .select(SocialAppSite::as_select())
        .first(conn)
}

pub fn insert_social_app_site(
    conn: &mut PgConnection,
    new: NewSocialAppSite,
) -> QueryResult<SocialAppSite> {
    diesel::insert_into(socialaccount_socialapp_sites::table)
        .values(&new)
        .returning(SocialAppSite::as_returning())
        .get_result(conn)
}

pub fn update_social_app_site(
    conn: &mut PgConnection,
    rid: i64,
    cs: SocialAppSiteUpdate,
) -> QueryResult<SocialAppSite> {
    diesel::update(socialaccount_socialapp_sites::table.find(rid))
        .set(&cs)
        .returning(SocialAppSite::as_returning())
        .get_result(conn)
}

pub fn delete_social_app_site(conn: &mut PgConnection, rid: i64) -> QueryResult<usize> {
    diesel::delete(socialaccount_socialapp_sites::table.find(rid)).execute(conn)
}

// ── socialaccount_socialtoken ──────────────────────────────────────────────────

pub fn list_social_tokens(
    conn: &mut PgConnection,
    account_id_filter: Option<i32>,
    app_id_filter: Option<i32>,
    lim: Option<i64>,
    offset: Option<i64>,
) -> QueryResult<(i64, Vec<SocialToken>)> {
    use crate::schema::socialaccount_socialtoken::dsl::*;

    let mut cq = socialaccount_socialtoken.into_boxed();
    if let Some(a) = account_id_filter {
        cq = cq.filter(account_id.eq(a));
    }
    if let Some(a) = app_id_filter {
        cq = cq.filter(app_id.eq(a));
    }
    let count: i64 = cq.select(count_star()).first(conn)?;

    let mut q = socialaccount_socialtoken.into_boxed();
    if let Some(a) = account_id_filter {
        q = q.filter(account_id.eq(a));
    }
    if let Some(a) = app_id_filter {
        q = q.filter(app_id.eq(a));
    }
    let results = q
        .order(id.desc())
        .limit(limit(lim))
        .offset(offset.unwrap_or(0))
        .select(SocialToken::as_select())
        .load(conn)?;

    Ok((count, results))
}

pub fn get_social_token(conn: &mut PgConnection, rid: i32) -> QueryResult<SocialToken> {
    socialaccount_socialtoken::table
        .find(rid)
        .select(SocialToken::as_select())
        .first(conn)
}

pub fn insert_social_token(
    conn: &mut PgConnection,
    new: NewSocialToken,
) -> QueryResult<SocialToken> {
    diesel::insert_into(socialaccount_socialtoken::table)
        .values(&new)
        .returning(SocialToken::as_returning())
        .get_result(conn)
}

pub fn update_social_token(
    conn: &mut PgConnection,
    rid: i32,
    cs: SocialTokenUpdate,
) -> QueryResult<SocialToken> {
    diesel::update(socialaccount_socialtoken::table.find(rid))
        .set(&cs)
        .returning(SocialToken::as_returning())
        .get_result(conn)
}

pub fn delete_social_token(conn: &mut PgConnection, rid: i32) -> QueryResult<usize> {
    diesel::delete(socialaccount_socialtoken::table.find(rid)).execute(conn)
}
