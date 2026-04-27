use crate::{
    common::read_file,
    metadata::{self, Meta},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;
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
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = read_file(path)?;
        let mut toml: MetaV1 = toml::from_str(&contents)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, path.display()))?;
        toml.fix()?;
        Ok(toml)
    }

    pub fn fix(&mut self) -> Result<()> {
        // Some confusion over dates as quoted strings or unquoted TOML values
        // But there's no JSON "date" format
        if let Datelike::TomlDate(dt) = self.initial.date {
            self.initial.date = Datelike::Stringy(dt.to_string())
        }

        if let Some(papers) = &mut self.papers {
            for paper in papers.iter_mut() {
                let new_volume = if let Numlike::TomlVal(val) = &paper.volume {
                    Some(match val {
                        TomlValue::String(v) => Numlike::Stringy(v.to_string()),
                        TomlValue::Integer(v) => Numlike::Stringy(v.to_string()),
                        TomlValue::Float(v) => Numlike::Stringy(v.to_string()),
                        _ => Numlike::Stringy("".to_string()),
                    })
                } else {
                    None
                };
                if let Some(v) = new_volume {
                    paper.volume = v;
                }

                let new_number = if let Some(Numlike::TomlVal(n)) = &paper.number {
                    Some(Some(match n {
                        TomlValue::String(v) => Numlike::Stringy(v.to_string()),
                        TomlValue::Integer(v) => Numlike::Stringy(v.to_string()),
                        TomlValue::Float(v) => Numlike::Stringy(v.to_string()),
                        _ => Numlike::Stringy("".to_string()),
                    }))
                } else {
                    None
                };
                if let Some(n) = new_number {
                    paper.number = n;
                }
            }
        }

        // Older versions of the TOML had separate fields for PDB/Uniprot
        for protein in &mut self.proteins {
            if protein.pdb_id.is_some() {
                protein.molecule_id_type = Some(MoleculeType::PDB);
                protein.molecule_id = protein.pdb_id.take();
            } else if protein.uniprot_id.is_some() {
                protein.molecule_id_type = Some(MoleculeType::Uniprot);
                protein.molecule_id = protein.uniprot_id.take();
            }
            protein.pdb_id = None;
            protein.uniprot_id = None;
        }
        Ok(())
    }

    pub fn to_v2(&self) -> Result<Meta> {
        let external_links = match &self.initial.external_link {
            Some(link) => Some(vec![metadata::ExternalLink {
                url: link.clone(),
                label: None,
            }]),
            _ => None,
        };

        let reqd = &self.required_files;
        let additional_files = self.additional_files.as_ref().map(|files| {
            files
                .iter()
                .map(|f| metadata::AdditionalFile {
                    file_name: f.file_name.clone(),
                    file_type: f.file_type.clone(),
                    description: f.description.clone(),
                })
                .collect::<Vec<_>>()
        });
        let (forcefield, forcefield_comments) = match &self.forcefield {
            Some(f) => (f.forcefield.clone(), f.forcefield_comments.clone()),
            _ => (None, None),
        };
        let protonation_method = match &self.protonation_method {
            Some(p) => p.protonation_method.clone(),
            _ => None,
        };

        let mut pdb_id: Option<String> = None;
        let mut uniprot_ids: Vec<String> = vec![];
        for protein in &self.proteins {
            if protein.molecule_id_type == Some(MoleculeType::PDB) {
                pdb_id = protein.molecule_id.clone();
            } else if protein.molecule_id_type == Some(MoleculeType::Uniprot) {
                if let Some(uniprot_id) = protein.molecule_id.clone() {
                    uniprot_ids.push(uniprot_id);
                }
            }
        }

        let water = if let Some(w) = &self.water {
            match (w.model.clone(), w.density) {
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
        if let Some(vals) = &self.ligands {
            for v in vals {
                ligands.push(metadata::Ligand {
                    name: v.name.clone(),
                    smiles: v.smiles.clone(),
                })
            }
        }

        let mut solutes = vec![];
        if let Some(vals) = &self.solvents {
            for v in vals {
                solutes.push(metadata::Solute {
                    name: v.name.clone(),
                    concentration_mol_liter: v.ion_concentration,
                })
            }
        }

        let mut papers = vec![];
        if let Some(vals) = &self.papers {
            for v in vals {
                papers.push(metadata::Paper {
                    title: v.title.clone(),
                    authors: v.authors.clone(),
                    journal: v.journal.clone(),
                    volume: v
                        .volume
                        .to_integer()
                        .ok_or_else(|| anyhow!("paper volume is not an integer"))?
                        as u32,
                    number: v.number.as_ref().and_then(|v| v.to_string()),
                    year: v.year,
                    pages: v.pages.clone(),
                    doi: v.doi.clone(),
                });
            }
        }

        let mut contributors = vec![];
        if let Some(vals) = &self.contributors {
            for v in vals {
                contributors.push(metadata::Contributor {
                    name: v.name.clone(),
                    email: v.email.clone(),
                    institution: v.institution.clone(),
                    orcid: v.orcid.clone(),
                });
            }
        }

        let temperature_kelvin = self
            .temperature
            .as_ref()
            .map_or(0, |t| t.temperature.unwrap_or(0));
        let integration_timestep_fs = self
            .timestep_information
            .as_ref()
            .map_or(0., |t| t.integration_time_step.unwrap_or(0.))
            as u32;

        Ok(Meta {
            mdrepo_id: None,
            lead_contributor_orcid: self.initial.lead_contributor_orcid.clone(),
            trajectory_file_name: reqd.trajectory_file_name.clone(),
            structure_file_name: reqd.structure_file_name.clone(),
            topology_file_name: reqd.topology_file_name.clone(),
            temperature_kelvin,
            integration_timestep_fs,
            short_description: self
                .initial
                .short_description
                .as_deref()
                .unwrap_or("")
                .to_string(),
            description: self.initial.description.clone(),
            software_name: self.software.name.clone(),
            software_version: self
                .software
                .version
                .as_deref()
                .unwrap_or("NA")
                .to_string(),
            toml_version: Some(2),
            user_accession: None,
            external_links,
            run_commands: self.initial.commands.clone(),
            additional_files,
            forcefield,
            forcefield_comments,
            protonation_method,
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
            solutes: if solutes.is_empty() {
                None
            } else {
                Some(solutes)
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
        })
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[cfg(test)]
mod metav1_tests {
    use super::MetaV1;
    use anyhow::Result;
    use std::path::Path;
    const INPUT1: &str = "tests/inputs/metadata/MDR00015378.v1.toml";

    #[test]
    fn from_file() -> Result<()> {
        let res = MetaV1::from_file(&Path::new(INPUT1));
        assert!(res.is_ok());
        let meta = res.expect("result");
        let short_desc = meta.initial.short_description.expect("short_desc");
        assert!(
            short_desc.starts_with("8 ns simulation of the 5aom PDB entry (P04637)")
        );

        let contributors = meta.contributors;
        assert!(contributors.is_some());

        let contributors = contributors.expect("contributors");
        assert_eq!(contributors.len(), 14);

        Ok(())
    }

    #[test]
    fn to_v2() -> Result<()> {
        let res = MetaV1::from_file(&Path::new(INPUT1));
        assert!(res.is_ok());
        let meta_v1 = res?;
        let meta_v2 = meta_v1.to_v2()?;

        assert!(
            meta_v2
                .short_description
                .starts_with("8 ns simulation of the 5aom PDB entry (P04637)")
        );

        let contributors = meta_v2.contributors;
        assert!(contributors.is_some());

        let contributors =
            contributors.ok_or_else(|| anyhow::anyhow!("no contributors"))?;
        assert_eq!(contributors.len(), 14);

        Ok(())
    }
}
