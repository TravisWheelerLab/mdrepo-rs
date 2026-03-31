use anyhow::{bail, Result};
use clap::Parser;
use dotenvy::dotenv;
use libmdrepo::{constants, metadata};
use mdr_db::ops;
use mdr_gen_meta::types::{Args, FileFormat, Server};
use std::{
    env,
    fs::File,
    io::{self, Write},
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
    let software = ops::get_software(&mut conn, sim.software_id.unwrap())?;
    let pdb = sim
        .pdb_id
        .map(|pdb_pk| ops::get_pdb(&mut conn, pdb_pk))
        .transpose()?;
    let (_, sim2uniprot) = ops::list_simulation_uniprots(
        &mut conn,
        Some(args.simulation_id),
        None,
        None,
        None,
    )?;
    let mut uniprot_ids: Vec<String> = vec![];
    for s2u in sim2uniprot {
        if let Ok(uniprot) = ops::get_uniprot(&mut conn, s2u.uniprot_id) {
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
                &mut conn,
                Some("orcid".to_string()),
                Some(user_id),
                None,
                None,
            )?;
            res.first().map(|v| v.uid.to_string())
        }
        _ => None,
    };

    let (_, up_files) = ops::list_uploaded_files(
        &mut conn,
        None,
        Some(args.simulation_id),
        None,
        None,
    )?;

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

    let (_, ligands_res) =
        ops::list_ligands(&mut conn, None, Some(args.simulation_id), None, None)?;

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

    let (_, solvents_res) =
        ops::list_solvents(&mut conn, None, Some(args.simulation_id), None, None)?;

    let solvents = if solvents_res.is_empty() {
        None
    } else {
        Some(
            solvents_res
                .iter()
                .map(|val| metadata::Solvent {
                    name: val.name.clone(),
                    concentration_mol_liter: val.concentration,
                })
                .collect::<Vec<_>>(),
        )
    };

    let (_, links_res) = ops::list_external_links(
        &mut conn,
        None,
        Some(args.simulation_id),
        None,
        None,
    )?;

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
        ops::list_contributions(&mut conn, None, Some(args.simulation_id), None, None)?;

    let contributors = if contributors_res.is_empty() {
        None
    } else {
        Some(
            contributors_res
                .iter()
                .map(|val| metadata::Contributor {
                    name: val.name.clone().unwrap(),
                    email: val.email.clone(),
                    institution: val.institution.clone(),
                    orcid: val.orcid.clone(),
                })
                .collect::<Vec<_>>(),
        )
    };

    let (_, sim2pub) = ops::list_simulation_pubs(
        &mut conn,
        Some(args.simulation_id),
        None,
        None,
        None,
    )?;
    let mut papers: Vec<metadata::Paper> = vec![];
    for s2p in sim2pub {
        if let Ok(val) = ops::get_pub(&mut conn, s2p.pub_id) {
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
        trajectory_file_name: trajectory_file_name.unwrap(),
        structure_file_name: structure_file_name.unwrap(),
        topology_file_name: topology_file_name.unwrap(),
        temperature_kelvin: sim.temperature.unwrap() as u32,
        integration_timestep_fs: sim.integration_timestep_fs.unwrap() as u32,
        short_description: sim.short_description.unwrap(),
        software_name: software.name,
        software_version: software.version.unwrap_or("".to_string()),
        mdrepo_id: Some(format!("MDR{:08}", args.simulation_id)),
        description: sim.description,
        toml_version: Some(constants::METADATA_TOML_VERSION),
        user_accession: sim.user_accession,
        run_commands: sim.run_commands,
        forcefield: sim.forcefield,
        forcefield_comments: sim.forcefield_comments,
        protonation_method: sim.protonation_method,
        replicate_id: None, // TODO fix?
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
        solvents,
        dois: None,
        papers: if papers.is_empty() {
            None
        } else {
            Some(papers)
        },
        external_links,
        contributors,
    };

    let errors = meta.check();
    if !errors.is_empty() {
        bail!("Errors!\n{}", errors.join("\n"));
    }

    let mut out_file = open_outfile(&args.outfile)?;
    let format = args.format.clone().unwrap_or(guess_format(&args.outfile));
    write!(
        out_file,
        "{}",
        if format == FileFormat::Json {
            meta.to_json()?
        } else {
            meta.to_toml()?
        }
    )?;

    Ok(())
}

// --------------------------------------------------
fn open_outfile(filename: &str) -> Result<Box<dyn Write>> {
    match filename {
        "-" => Ok(Box::new(io::stdout())),
        out_name => Ok(Box::new(File::create(out_name)?)),
    }
}

// --------------------------------------------------
fn guess_format(filename: &str) -> FileFormat {
    if filename == "-" {
        FileFormat::Toml
    } else {
        match Path::new(filename).extension() {
            Some(ext) => {
                if ext == "json" {
                    FileFormat::Json
                } else {
                    FileFormat::Toml
                }
            }
            _ => FileFormat::Toml,
        }
    }
}
