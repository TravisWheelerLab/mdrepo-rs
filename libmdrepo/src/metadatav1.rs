use crate::common::read_file;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::value::Value as TomlValue;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum Datelike {
    Stringy(String),
    TomlDate(toml::value::Datetime),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Numlike {
    Stringy(String),
    TomlVal(TomlValue),
}

impl Numlike {
    pub fn to_integer(&self) -> Option<i64> {
        match self {
            Numlike::Stringy(val) => val.parse().ok(),
            Numlike::TomlVal(toml_val) => match toml_val {
                TomlValue::String(val) => val.parse().ok(),
                TomlValue::Integer(val) => Some(*val),
                TomlValue::Float(val) => format!("{}", val.round()).parse::<i64>().ok(),
                _ => None,
            },
        }
    }

    pub fn to_string(&self) -> Option<String> {
        match self {
            Numlike::Stringy(val) => Some(val.clone()),
            Numlike::TomlVal(toml_val) => match toml_val {
                TomlValue::String(val) => Some(val.clone()),
                TomlValue::Integer(val) => Some(val.to_string()),
                TomlValue::Float(val) => Some(val.to_string()),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum MoleculeType {
    #[serde(alias = "Pdb")]
    PDB,

    #[serde(alias = "UNIPROT")]
    Uniprot,

    Other(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetaV1 {
    pub mdrepo_id: Option<String>,

    pub initial: Initial,

    pub software: Software,

    pub required_files: RequiredFile,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_files: Option<Vec<AdditionalFile>>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub proteins: Vec<Protein>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicates: Option<Replicates>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<Water>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ligands: Option<Vec<Ligand>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub solvents: Option<Vec<Solvent>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield: Option<Forcefield>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub protonation_method: Option<Protonation>,

    #[serde(alias = "timestep")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestep_information: Option<Timestep>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub papers: Option<Vec<Paper>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<Vec<Contributor>>,

    #[serde(
        alias = "simulation_permissions",
        skip_serializing_if = "Option::is_none"
    )]
    pub permissions: Option<Vec<Permission>>,

    pub pdb_id: Option<String>,
}

impl MetaV1 {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = read_file(path)?;
        let mut toml: MetaV1 = toml::from_str(&contents)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, path.display()))?;
        toml.fix();
        Ok(toml)
    }

    pub fn fix(&mut self) {
        // Some confusion over dates as quoted strings or unquoted TOML values
        // But there's no JSON "date" format
        if let Datelike::TomlDate(dt) = self.initial.date {
            self.initial.date = Datelike::Stringy(dt.to_string())
        }

        if let Some(papers) = &self.papers {
            let new_papers: Vec<_> = papers
                .iter()
                .map(|paper| {
                    let volume = if let Numlike::TomlVal(val) = &paper.volume {
                        match val {
                            TomlValue::String(v) => Numlike::Stringy(v.to_string()),
                            TomlValue::Integer(v) => Numlike::Stringy(v.to_string()),
                            TomlValue::Float(v) => Numlike::Stringy(v.to_string()),
                            _ => Numlike::Stringy("".to_string()),
                        }
                    } else {
                        paper.volume.clone()
                    };

                    let number = paper.number.clone().map(|val| {
                        if let Numlike::TomlVal(n) = val {
                            
                            match n {
                                TomlValue::String(v) => Numlike::Stringy(v.to_string()),
                                TomlValue::Integer(v) => {
                                    Numlike::Stringy(v.to_string())
                                }
                                TomlValue::Float(v) => Numlike::Stringy(v.to_string()),
                                _ => Numlike::Stringy("".to_string()),
                            }
                        } else {
                            val.clone()
                        }
                    });

                    let mut new_paper = paper.clone();
                    new_paper.volume = volume;
                    new_paper.number = number;
                    new_paper
                })
                .collect();

            self.papers = Some(new_papers);
        }

        // TODO: fix
        // Older versions of the TOML had separate fields for PDB/Uniprot
        let new_proteins: Vec<_> = self
            .proteins
            .iter()
            .map(|protein| {
                if let Some(pdb_id) = &protein.pdb_id {
                    Protein {
                        molecule_id_type: Some(MoleculeType::PDB),
                        molecule_id: Some(pdb_id.to_string()),
                        pdb_id: None,
                        uniprot_id: None,
                    }
                } else if let Some(uniprot_id) = &protein.uniprot_id {
                    Protein {
                        molecule_id_type: Some(MoleculeType::Uniprot),
                        molecule_id: Some(uniprot_id.to_string()),
                        pdb_id: None,
                        uniprot_id: None,
                    }
                } else {
                    Protein {
                        molecule_id_type: protein.molecule_id_type.clone(),
                        molecule_id: protein.molecule_id.clone(),
                        pdb_id: None,
                        uniprot_id: None,
                    }
                }
            })
            .collect();

        self.proteins = new_proteins;
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Initial {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_link: Option<String>,

    pub lead_contributor_orcid: String,

    pub date: Datelike,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub simulation_is_restricted: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ns: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AdditionalFile {
    #[serde(alias = "additional_file_type")]
    pub file_type: String,

    #[serde(alias = "additional_file_name")]
    pub file_name: String,

    #[serde(alias = "additional_file_description")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Contributor {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Forcefield {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield_comments: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Permission {
    user_orcid: String,
    can_edit: bool,
    can_view: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Protonation {
    pub protonation_method: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Timestep {
    pub integration_time_step: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Paper {
    pub title: String,

    pub authors: String,

    pub journal: String,

    pub volume: Numlike,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<Numlike>,

    pub year: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,

    pub doi: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Temperature {
    pub temperature: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Ligand {
    pub name: String,
    pub smiles: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RequiredFile {
    pub trajectory_file_name: String,
    pub structure_file_name: String,
    pub topology_file_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Software {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Replicates {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_replicates: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicate: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Protein {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub molecule_id_type: Option<MoleculeType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub molecule_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdb_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uniprot_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Solvent {
    pub name: String,

    #[serde(alias = "salt_concentration")]
    pub ion_concentration: f64,

    #[serde(alias = "solvent_concentration_units")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concentration_units: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Water {
    pub is_present: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_density_units: Option<String>,
}
