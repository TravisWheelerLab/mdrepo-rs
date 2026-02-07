use crate::common::read_file;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, NoneAsEmptyString};
use std::path::PathBuf;
use toml::value::Value as TomlValue;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
enum Datelike {
    Stringy(String),
    TomlDate(toml::value::Datetime),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
enum Numlike {
    Stringy(String),
    TomlVal(TomlValue),
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
pub struct Meta {
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
}

impl Meta {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = read_file(path)?;
        let mut toml: Meta = toml::from_str(&contents)?;
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
                .into_iter()
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
                            let new_number = match n {
                                TomlValue::String(v) => Numlike::Stringy(v.to_string()),
                                TomlValue::Integer(v) => {
                                    Numlike::Stringy(v.to_string())
                                }
                                TomlValue::Float(v) => Numlike::Stringy(v.to_string()),
                                _ => Numlike::Stringy("".to_string()),
                            };
                            new_number
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
        //let new_proteins: Vec<_> = self
        //    .proteins
        //    .into_iter()
        //    .map(|protein| {
        //        if let Some(pdb_id) = &protein.pdb_id {
        //            Protein {
        //                molecule_id_type: Some(MoleculeType::PDB),
        //                molecule_id: Some(pdb_id.to_string()),
        //                pdb_id: None,
        //                uniprot_id: None,
        //            }
        //        } else if let Some(uniprot_id) = &protein.uniprot_id {
        //            Protein {
        //                molecule_id_type: Some(MoleculeType::Uniprot),
        //                molecule_id: Some(uniprot_id.to_string()),
        //                pdb_id: None,
        //                uniprot_id: None,
        //            }
        //        } else {
        //            Protein {
        //                molecule_id_type: protein.molecule_id_type.clone(),
        //                molecule_id: protein.molecule_id.clone(),
        //                pdb_id: None,
        //                uniprot_id: None,
        //            }
        //        }
        //    })
        //    .collect();

        //self.proteins = new_proteins;
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct Initial {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub short_description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub external_link: Option<String>,

    pub lead_contributor_orcid: String,

    pub date: Datelike,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
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
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct Contributor {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub orcid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub institution: Option<String>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct Forcefield {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "NoneAsEmptyString")]
    forcefield: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    forcefield_comments: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Permission {
    user_orcid: String,
    can_edit: bool,
    can_view: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Protonation {
    protonation_method: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Timestep {
    integration_time_step: Option<f64>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Paper {
    title: String,

    authors: String,

    journal: String,

    volume: Numlike,

    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<Numlike>,

    year: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    //[serde_as(as = "NoneAsEmptyString")]
    pages: Option<String>,
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

#[derive(Debug, Deserialize, Serialize)]
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
    pub is_present: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_density_units: Option<String>,
}

#[cfg(test)]
mod tests {
    const INPUT1: &str = "tests/inputs/MDR_00000002.toml";
    const INPUT2: &str = "tests/inputs/MDR_00004423.toml";
    use super::{Datelike, Ligand, Meta, Protein};
    use anyhow::Result;
    use std::fs;
    use toml;

    #[test]
    fn t1() -> Result<()> {
        let toml = fs::read_to_string(INPUT1)?;
        let mut meta: Meta = toml::from_str(&toml)?;
        meta.fix();

        assert_eq!(
            meta.initial.date,
            Datelike::Stringy("2020-07-13".to_string())
        );

        assert!(meta.proteins.is_some());
        let proteins = meta.proteins.unwrap();
        assert_eq!(proteins.len(), 1);
        assert_eq!(
            proteins[0],
            Protein {
                molecule_id_type: Some(MoleculeType::PDB),
                molecule_id: Some("1U19.A".to_string()),
                pdb_id: None,
                uniprot_id: None,
            }
        );

        assert!(meta.solvents.is_some());
        let solvents = meta.solvents.unwrap();
        assert_eq!(solvents.len(), 2);

        assert!(meta.papers.is_some());
        let papers = meta.papers.unwrap();
        assert_eq!(papers.len(), 2);

        assert!(meta.contributors.is_some());
        let contributors = meta.contributors.unwrap();
        assert_eq!(contributors.len(), 1);

        assert!(meta.replicates.is_some());
        let replicates = meta.replicates.unwrap();
        assert_eq!(replicates.replicate, Some(2));
        assert_eq!(replicates.total_replicates, Some(3));

        assert_eq!(meta.software.name, "ACEMD".to_string());
        assert_eq!(meta.software.version, Some("GPUGRID".to_string()));

        assert!(meta.water.is_some());
        let water = meta.water.unwrap();
        assert_eq!(water.is_present, true);

        Ok(())
    }

    #[test]
    fn t2() -> Result<()> {
        let toml = fs::read_to_string(INPUT2)?;
        let mut meta: Meta = toml::from_str(&toml)?;
        meta.fix();

        assert_eq!(
            meta.initial.date,
            Datelike::Stringy("2024-09-20".to_string())
        );

        assert!(meta.initial.commands.is_some());
        let commands = meta.initial.commands.unwrap();
        assert!(commands.starts_with("gmx_mpi"));
        assert!(commands.ends_with("gpu"));

        assert!(meta.replicates.is_some());
        let replicates = meta.replicates.unwrap();
        assert_eq!(replicates.replicate, Some(1));
        assert_eq!(replicates.total_replicates, Some(4));

        assert!(meta.proteins.is_some());
        let proteins = meta.proteins.unwrap();
        assert_eq!(proteins.len(), 1);
        assert_eq!(
            proteins[0],
            Protein {
                molecule_id_type: Some(MoleculeType::PDB),
                molecule_id: Some("5UPE".to_string()),
                pdb_id: None,
                uniprot_id: None,
            }
        );

        assert!(meta.ligands.is_some());
        let ligands = meta.ligands.unwrap();
        assert_eq!(ligands.len(), 1);
        assert_eq!(
            ligands[0],
            Ligand {
                name:
                    "N-{4-[(3-phenylpropyl)carbamoyl]phenyl}-2H-isoindole-2-carboxamide"
                        .to_string(),
                smiles: "c1ccc(cc1)CCCNC(=O)c2ccc(cc2)NC(=O)n3cc4ccccc4c3".to_string()
            }
        );

        assert!(meta.solvents.is_some());
        let solvents = meta.solvents.unwrap();
        assert_eq!(solvents.len(), 1);

        assert!(meta.papers.is_none());

        assert!(meta.contributors.is_some());
        let contributors = meta.contributors.unwrap();
        assert_eq!(contributors.len(), 1);

        assert!(meta.forcefield.is_some());
        let forcefield = meta.forcefield.unwrap();
        assert_eq!(forcefield.forcefield, Some("charmm36".to_string()));
        assert_eq!(
            forcefield.forcefield_comments,
            Some("ligand parameters from swissparam".to_string())
        );

        assert!(meta.temperature.is_some());
        let temperature = meta.temperature.unwrap();
        assert_eq!(temperature.temperature, Some(300));

        assert_eq!(meta.software.name, "GROMACS".to_string());
        assert_eq!(meta.software.version, Some("2024".to_string()));

        assert!(meta.water.is_some());
        let water = meta.water.unwrap();
        assert_eq!(water.is_present, true);
        assert_eq!(water.model, Some("TIP3P".to_string()));
        assert_eq!(water.density, Some(1000.));
        assert_eq!(water.water_density_units, Some("g/m^3".to_string()));

        //assert!(meta.required_files.is_some());
        let required_files = meta.required_files;
        assert_eq!(required_files.trajectory_file_name, "prodw.xtc".to_string());
        assert_eq!(
            required_files.structure_file_name,
            "prod.part0135.pdb".to_string()
        );
        assert_eq!(required_files.topology_file_name, "prod.tpr".to_string());

        assert!(meta.additional_files.is_some());
        let additional_files = meta.additional_files.unwrap();
        assert_eq!(additional_files.len(), 9);

        Ok(())
    }
}
