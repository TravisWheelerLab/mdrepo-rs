use anyhow::{bail, Result};
use clap::Parser;
use dotenvy::dotenv;
//use libmdrepo::metadata::Meta;
use mdr_db::ops;
use mdr_gen_meta::types::{Args, Server};
use std::env;

//const DEFAULT_ORCID: &str = "0000-0000-0000-0000";

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Args) -> Result<()> {
    let url = match args.dsn {
        Some(val) => val,
        _ => {
            dotenv().ok();
            let env_key = match args.server {
                Server::Production => "PRODUCTION_DSN",
                Server::Staging => "STAGING_DSN",
            };
            match env::var(env_key) {
                Ok(val) => val,
                Err(e) => bail!("{env_key}: {e}"),
            }
        }
    };
    let mut conn = mdr_db::connect(&url);
    let sim = ops::get_simulation(&mut conn, args.simulation_id)?;
    let lead_contributor_orcid = match sim.created_by_id {
        Some(user_id) => {
            let (_count, res) = ops::list_social_accounts(
                &mut conn,
                Some("orcid".to_string()),
                Some(user_id),
                None,
                None,
            )?;
            dbg!(&res);
            res.first().map(|v| v.uid.to_string())
        }
        _ => None,
    };
    dbg!(&lead_contributor_orcid);

    //dbg!(&sim);
    //let lead_contributor_orcid =
    //    sim.created_by_id.map(|id| ops::get_user(&mut conn, id));
    //dbg!(&lead_contributor_orcid);

    //let up_files = ops::list_uploaded_files(
    //    &mut conn,
    //    None,
    //    Some(args.simulation_id),
    //    None,
    //    None,
    //)?;
    //dbg!(&up_files);

    //let meta = Meta {
    //    lead_contributor_orcid: sim.,
    //    trajectory_file_name: "",
    //    structure_file_name: "",
    //    topology_file_name: "",
    //    temperature_kelvin: "",
    //    integration_timestep_fs: "",
    //    short_description: "",
    //    software_name: "",
    //    software_version: "",
    //    mdrepo_id: "",
    //    description: "",
    //    toml_version: "",
    //    user_accession: "",
    //    run_commands: "",
    //    forcefield: "",
    //    forcefield_comments: "",
    //    protonation_method: "",
    //    replicate_id: "",
    //    pdb_id: "",
    //    uniprot_ids: "",
    //    water: "",
    //    additional_files: "",
    //    ligands: "",
    //    solvents: "",
    //    dois: "",
    //    papers: "",
    //    external_links: "",
    //    contributors: "",
    //};
    Ok(())
}
