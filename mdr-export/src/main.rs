use anyhow::{Result, anyhow, bail};
use clap::Parser;
use diesel::pg::PgConnection;
use dotenvy::dotenv;
use libmdrepo::{constants, metadata};
use mdr_db::ops;
use mdr_export::types::{Args, Server};
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::Path,
};

const DEFAULT_ORCID: &str = "0000-0000-0000-0000";

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Args) -> Result<()> {
    dotenv().ok();
    let env_key = match args.server {
        Server::Production => "PRODUCTION_DSN",
        Server::Staging => "STAGING_DSN",
    };
    let url = match env::var(env_key) {
        Ok(val) => val,
        Err(e) => bail!("{env_key}: {e}"),
    };
    let out_dir = Path::new(&args.out_dir);
    if !out_dir.exists() {
        fs::create_dir(&out_dir)?;
    }

    let mut conn = mdr_db::connect(&url)?;

    let simulation_ids = if args.simulation_ids.is_empty() {
        ops::get_all_simulation_ids(&mut conn)?
    } else {
        args.simulation_ids
    };

    for (i, sim_id) in simulation_ids.into_iter().enumerate() {
        let mdrepo_id = format!("MDR{sim_id:08}");
        let out_file = out_dir.join(format!("{}.json", mdrepo_id));
        if !out_file.exists() {
            println!("{:6}: {mdrepo_id}", i + 1);
            match get_sim(&mut conn, sim_id) {
                Ok(meta) => {
                    let mut out_fh = File::create(&out_file)?;
                    write!(out_fh, "{}", meta.to_json()?)?;
                }
                Err(e) => {
                    eprintln!("{e}");
                }
            }
        }
    }

    Ok(())
}

// --------------------------------------------------
fn empty_to_none(val: Option<String>) -> Option<String> {
    val.map_or(None, |v| if v.is_empty() { None } else { Some(v.clone()) })
}

// --------------------------------------------------
fn get_sim(conn: &mut PgConnection, sim_id: i64) -> Result<metadata::Meta> {
    let sim = ops::get_simulation(conn, sim_id)?;
    let software = ops::get_software(
        conn,
        sim.software_id
            .ok_or_else(|| anyhow!("simulation has no software_id"))?,
    )?;
    let pdb = sim
        .pdb_id
        .map(|pdb_pk| ops::get_pdb(conn, pdb_pk))
        .transpose()?;
    let (_, sim2uniprot) =
        ops::list_simulation_uniprots(conn, Some(sim_id), None, None, None)?;
    let mut uniprot_ids: Vec<String> = vec![];
    for s2u in sim2uniprot {
        if let Ok(uniprot) = ops::get_uniprot(conn, s2u.uniprot_id) {
            uniprot_ids.push(uniprot.uniprot_id.to_string())
        }
    }

    let water = match (&sim.water_type, &sim.water_density) {
        (Some(model), Some(density_kg_m3)) => Some(metadata::Water {
            model: model.clone(),
            density_kg_m3: *density_kg_m3,
        }),
        _ => None,
    };

    let lead_contributor_orcid = match sim.created_by_id {
        Some(user_id) => {
            let (_count, res) = ops::list_social_accounts(
                conn,
                Some("orcid".to_string()),
                Some(user_id),
                None,
                None,
            )?;
            res.first().map(|v| v.uid.to_string())
        }
        _ => None,
    };

    let (_, up_files) = ops::list_uploaded_files(conn, None, Some(sim_id), None, None)?;

    let mut additional_files: Vec<metadata::AdditionalFile> = vec![];
    let mut trajectory_file_name: Option<String> = None;
    let mut structure_file_name: Option<String> = None;
    let mut topology_file_name: Option<String> = None;

    for file in up_files {
        if file.is_primary {
            match file.file_type.as_str() {
                "Trajectory" => {
                    if trajectory_file_name.is_none() {
                        trajectory_file_name = Some(file.filename.clone())
                    } else {
                        bail!("Found multiple primary trajectory files");
                    }
                }
                "Structure" => {
                    if structure_file_name.is_none() {
                        structure_file_name = Some(file.filename.clone())
                    } else {
                        bail!("Found multiple primary structure files");
                    }
                }
                "Topology" => {
                    if topology_file_name.is_none() {
                        topology_file_name = Some(file.filename.clone())
                    } else {
                        bail!("Found multiple primary topology files");
                    }
                }
                _ => bail!(
                    r#"Primary file type "{}" is not Trajectory/Structure/Topology"#,
                    file.file_type
                ),
            }
        } else {
            additional_files.push(metadata::AdditionalFile {
                file_name: file.filename.clone(),
                file_type: file.file_type.clone(),
                description: file.description.clone(),
            })
        }
    }

    for (field, val) in [
        ("trajectory_file_name", &trajectory_file_name),
        ("structure_file_name", &structure_file_name),
        ("topology_file_name", &topology_file_name),
    ] {
        if val.is_none() {
            bail!(r#""{field}" has no value"#)
        }
    }

    let (_, ligands_res) = ops::list_ligands(conn, None, Some(sim_id), None, None)?;

    let ligands = if ligands_res.is_empty() {
        None
    } else {
        Some(
            ligands_res
                .iter()
                .map(|val| metadata::Ligand {
                    name: val.name.clone(),
                    smiles: val.smiles_string.clone(),
                })
                .collect::<Vec<_>>(),
        )
    };

    let (_, solutes_res) = ops::list_solutes(conn, None, Some(sim_id), None, None)?;

    let solutes = if solutes_res.is_empty() {
        None
    } else {
        Some(
            solutes_res
                .iter()
                .map(|val| metadata::Solute {
                    name: val.name.clone(),
                    concentration_mol_liter: val.concentration,
                })
                .collect::<Vec<_>>(),
        )
    };

    let (_, links_res) =
        ops::list_external_links(conn, None, Some(sim_id), None, None)?;

    let external_links = if links_res.is_empty() {
        None
    } else {
        Some(
            links_res
                .iter()
                .map(|val| metadata::ExternalLink {
                    url: val.url.clone(),
                    label: val.label.clone(),
                })
                .collect::<Vec<_>>(),
        )
    };

    let (_, contributors_res) =
        ops::list_contributions(conn, None, Some(sim_id), None, None)?;

    let contributors = if contributors_res.is_empty() {
        None
    } else {
        Some(
            contributors_res
                .iter()
                .map(|val| -> Result<metadata::Contributor> {
                    Ok(metadata::Contributor {
                        name: val
                            .name
                            .clone()
                            .ok_or_else(|| anyhow!("contributor has no name"))?,
                        email: empty_to_none(val.email.clone()),
                        institution: empty_to_none(val.institution.clone()),
                        orcid: empty_to_none(val.orcid.clone()),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )
    };

    let (_, sim2pub) = ops::list_simulation_pubs(conn, Some(sim_id), None, None, None)?;
    let mut papers: Vec<metadata::Paper> = vec![];
    for s2p in sim2pub {
        if let Ok(val) = ops::get_pub(conn, s2p.pub_id) {
            papers.push(metadata::Paper {
                title: val.title.clone(),
                authors: val.authors.clone(),
                journal: val.journal.clone(),
                volume: val.volume as u32,
                number: val.number.clone(),
                year: val.year as u32,
                pages: val.pages.clone(),
                doi: Some(val.doi),
            });
        }
    }

    let meta = metadata::Meta {
        lead_contributor_orcid: lead_contributor_orcid
            .unwrap_or(DEFAULT_ORCID.to_string()),
        trajectory_file_name: trajectory_file_name
            .ok_or_else(|| anyhow!("simulation has no trajectory file"))?,
        structure_file_name: structure_file_name
            .ok_or_else(|| anyhow!("simulation has no structure file"))?,
        topology_file_name: topology_file_name
            .ok_or_else(|| anyhow!("simulation has no topology file"))?,
        temperature_kelvin: sim
            .temperature
            .ok_or_else(|| anyhow!("simulation has no temperature"))?
            as u32,
        integration_timestep_fs: sim
            .integration_timestep_fs
            .ok_or_else(|| anyhow!("simulation has no integration timestep"))?
            as u32,
        short_description: sim
            .short_description
            .ok_or_else(|| anyhow!("simulation has no short description"))?,
        software_name: software.name,
        software_version: software.version.unwrap_or("".to_string()),
        mdrepo_id: Some(format!("MDR{sim_id:08}")),
        description: sim.description,
        toml_version: Some(constants::METADATA_TOML_VERSION),
        alias: sim.alias,
        run_commands: sim.run_commands,
        forcefield: sim.forcefield,
        forcefield_comments: sim.forcefield_comments,
        protonation_method: sim.protonation_method,
        pdb_id: pdb.map(|val| val.pdb_id),
        uniprot_ids: if uniprot_ids.is_empty() {
            None
        } else {
            Some(uniprot_ids)
        },
        water,
        additional_files: if additional_files.is_empty() {
            None
        } else {
            Some(additional_files)
        },
        ligands,
        solutes,
        dois: None,
        papers: if papers.is_empty() {
            None
        } else {
            Some(papers)
        },
        external_links,
        contributors,
    };
    Ok(meta)
}
