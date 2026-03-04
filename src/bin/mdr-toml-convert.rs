use anyhow::{bail, Result};
use clap::Parser;
use mdr::{
    metadata::{self, Meta},
    metadatav1::{self, MetaV1},
};
use std::{
    fs::File,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser, Debug)]
/// Convert MDRepo TOML v1 to v2
pub struct Args {
    /// Input TOML file
    #[arg(value_name = "FILE")]
    pub filename: PathBuf,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    outfile: String,
}

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Args) -> Result<()> {
    if &args.filename == &args.outfile {
        bail!("Will not overwrite input file with output file")
    }

    if Meta::from_file(&args.filename).is_ok() {
        bail!(r#""{}" is already in v2 format"#, &args.filename.display());
    }

    let v1 = MetaV1::from_file(&args.filename)?;
    let external_links = match &v1.initial.external_link {
        Some(link) => Some(vec![metadata::ExternalLink {
            url: link.clone(),
            label: None,
        }]),
        _ => None,
    };
    let reqd = v1.required_files;
    let additional_files = v1.additional_files.map(|files| {
        files
            .iter()
            .map(|f| metadata::AdditionalFile {
                file_name: f.file_name.clone(),
                file_type: f.file_type.clone(),
                description: f.description.clone(),
            })
            .collect::<Vec<_>>()
    });
    let (forcefield, forcefield_comments) = match v1.forcefield {
        Some(f) => (f.forcefield, f.forcefield_comments),
        _ => (None, None),
    };
    let protonation_method = match v1.protonation_method {
        Some(p) => p.protonation_method,
        _ => None,
    };

    let mut pdb_id: Option<String> = None;
    let mut uniprot_ids: Vec<String> = vec![];
    for protein in v1.proteins {
        if protein.molecule_id_type == Some(metadatav1::MoleculeType::PDB) {
            pdb_id = protein.molecule_id;
        } else if protein.molecule_id_type == Some(metadatav1::MoleculeType::Uniprot) {
            if let Some(uniprot_id) = protein.molecule_id {
                uniprot_ids.push(uniprot_id);
            }
        }
    }

    let water = if let Some(w) = v1.water {
        match (w.model, w.density) {
            (Some(model), Some(density_kg_m3)) => Some(metadata::Water {
                model,
                density_kg_m3,
            }),
            _ => None,
        }
    } else {
        None
    };

    let mut ligands = vec![];
    if let Some(vals) = v1.ligands {
        for v in vals {
            ligands.push(metadata::Ligand {
                name: v.name,
                smiles: v.smiles,
            })
        }
    }

    let mut solvents = vec![];
    if let Some(vals) = v1.solvents {
        for v in vals {
            solvents.push(metadata::Solvent {
                name: v.name,
                concentration_mol_liter: v.ion_concentration,
            })
        }
    }

    let mut papers = vec![];
    if let Some(vals) = v1.papers {
        for v in vals {
            papers.push(metadata::Paper {
                title: v.title,
                authors: v.authors,
                journal: v.journal,
                volume: v.volume.to_integer().unwrap() as u32,
                number: v.number.unwrap().to_string(),
                year: v.year,
                pages: v.pages,
                doi: v.doi,
            });
        }
    }

    let mut contributors = vec![];
    if let Some(vals) = v1.contributors {
        for v in vals {
            contributors.push(metadata::Contributor {
                name: v.name,
                email: v.email,
                institution: v.institution,
                orcid: v.orcid,
            });
        }
    }

    let meta = Meta {
        mdrepo_id: None,
        lead_contributor_orcid: v1.initial.lead_contributor_orcid.clone(),
        trajectory_file_name: reqd.trajectory_file_name.clone(),
        structure_file_name: reqd.structure_file_name.clone(),
        topology_file_name: reqd.topology_file_name.clone(),
        temperature_kelvin: v1.temperature.unwrap().temperature.unwrap(),
        integration_timestep_fs: v1
            .timestep_information
            .unwrap()
            .integration_time_step
            .unwrap() as u32,
        short_description: v1.initial.short_description.unwrap_or("".to_string()),
        description: v1.initial.description,
        software_name: v1.software.name,
        software_version: v1.software.version.unwrap_or("NA".to_string()),
        toml_version: Some(2),
        user_accession: None,
        external_links,
        run_commands: v1.initial.commands,
        additional_files,
        forcefield,
        forcefield_comments,
        protonation_method,
        replicate_id: None,
        pdb_id,
        dois: None,
        uniprot_ids: if uniprot_ids.is_empty() {
            None
        } else {
            Some(uniprot_ids)
        },
        water,
        ligands: if ligands.is_empty() {
            None
        } else {
            Some(ligands)
        },
        solvents: if solvents.is_empty() {
            None
        } else {
            Some(solvents)
        },
        papers: if papers.is_empty() {
            None
        } else {
            Some(papers)
        },
        contributors: if contributors.is_empty() {
            None
        } else {
            Some(contributors)
        },
    };

    let mut out_file = open_outfile(&args.outfile)?;
    write!(out_file, "{}", meta.to_toml()?)?;
    println!(r#"Done, wrote to "{}""#, &args.outfile);
    Ok(())
}

// --------------------------------------------------
fn open_outfile(filename: &str) -> Result<Box<dyn Write>> {
    match filename {
        "-" => Ok(Box::new(io::stdout())),
        out_name => Ok(Box::new(File::create(out_name)?)),
    }
}
