use crate::common::read_file;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toml::value::Value as TomlValue;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Numlike {
    Stringy(String),
    TomlVal(TomlValue),
}

impl Numlike {
    pub fn from_integer(val: i64) -> Self {
        Numlike::TomlVal(TomlValue::Integer(val))
    }

    pub fn to_integer(&self) -> Option<i64> {
        match self {
            Numlike::Stringy(val) => val.parse().ok(),
            Numlike::TomlVal(toml_val) => match toml_val {
                TomlValue::String(val) => val.parse().ok(),
                TomlValue::Integer(val) => Some(val.clone()),
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub mdrepo_id: Option<String>,

    pub lead_contributor_orcid: String,

    pub trajectory_file_name: String,

    pub structure_file_name: String,

    pub topology_file_name: String,

    pub temperature_kelvin: u32,

    pub integration_timestep_fs: f64,

    pub short_description: String,

    pub software_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub toml_version: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_accession: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_links: Option<Vec<ExternalLink>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_files: Option<Vec<AdditionalFile>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield_comments: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub protonation_method: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicate_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdb_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uniprot_ids: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<Water>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ligands: Option<Vec<Ligand>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub solvents: Option<Vec<Solvent>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub papers: Option<Vec<Paper>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dois: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<Vec<Contributor>>,
}

impl Meta {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = read_file(path)?;
        let mut toml: Meta = toml::from_str(&contents)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, path.display()))?;
        toml.fix();
        Ok(toml)
    }

    pub fn fix(&mut self) {
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
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExternalLink {
    pub url: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AdditionalFile {
    pub file_type: String,

    pub file_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Contributor {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Ligand {
    pub name: String,
    pub smiles: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Solvent {
    pub name: String,
    pub concentration_mol_liter: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Water {
    pub model: String,
    pub density_kg_m3: f32,
}

//#[cfg(test)]
//mod tests {
//    const INPUT1: &str = "tests/inputs/MDR_00000002.toml";
//    const INPUT2: &str = "tests/inputs/MDR_00004423.toml";
//    use super::{Datelike, Ligand, Meta, MoleculeType, Protein};
//    use anyhow::Result;
//    use std::fs;
//    //use toml;

//    #[test]
//    fn t1() -> Result<()> {
//        let toml = fs::read_to_string(INPUT1)?;
//        let mut meta: Meta = toml::from_str(&toml)?;
//        meta.fix();

//        assert_eq!(
//            meta.initial.date,
//            Datelike::Stringy("2020-07-13".to_string())
//        );

//        assert!(!meta.proteins.is_empty());
//        let proteins = meta.proteins;
//        assert_eq!(proteins.len(), 1);
//        assert_eq!(
//            proteins[0],
//            Protein {
//                molecule_id_type: Some(MoleculeType::PDB),
//                molecule_id: Some("1U19.A".to_string()),
//                pdb_id: None,
//                uniprot_id: None,
//            }
//        );

//        assert!(meta.solvents.is_some());
//        let solvents = meta.solvents.unwrap();
//        assert_eq!(solvents.len(), 2);

//        assert!(meta.papers.is_some());
//        let papers = meta.papers.unwrap();
//        assert_eq!(papers.len(), 2);

//        assert!(meta.contributors.is_some());
//        let contributors = meta.contributors.unwrap();
//        assert_eq!(contributors.len(), 1);

//        assert!(meta.replicates.is_some());
//        let replicates = meta.replicates.unwrap();
//        assert_eq!(replicates.replicate, Some(2));
//        assert_eq!(replicates.total_replicates, Some(3));

//        assert_eq!(meta.software.name, "ACEMD".to_string());
//        assert_eq!(meta.software.version, Some("GPUGRID".to_string()));

//        //assert!(meta.water.is_some());
//        //let water = meta.water.unwrap();
//        //assert_eq!(water.is_present, true);

//        Ok(())
//    }

//    #[test]
//    fn t2() -> Result<()> {
//        let toml = fs::read_to_string(INPUT2)?;
//        let mut meta: Meta = toml::from_str(&toml)?;
//        meta.fix();

//        assert_eq!(
//            meta.initial.date,
//            Datelike::Stringy("2024-09-20".to_string())
//        );

//        assert!(meta.initial.commands.is_some());
//        let commands = meta.initial.commands.unwrap();
//        assert!(commands.starts_with("gmx_mpi"));
//        assert!(commands.ends_with("gpu"));

//        assert!(meta.replicates.is_some());
//        let replicates = meta.replicates.unwrap();
//        assert_eq!(replicates.replicate, Some(1));
//        assert_eq!(replicates.total_replicates, Some(4));

//        assert!(meta.proteins.is_some());
//        let proteins = meta.proteins.unwrap();
//        assert_eq!(proteins.len(), 1);
//        assert_eq!(
//            proteins[0],
//            Protein {
//                molecule_id_type: Some(MoleculeType::PDB),
//                molecule_id: Some("5UPE".to_string()),
//                pdb_id: None,
//                uniprot_id: None,
//            }
//        );

//        assert!(meta.ligands.is_some());
//        let ligands = meta.ligands.unwrap();
//        assert_eq!(ligands.len(), 1);
//        assert_eq!(
//            ligands[0],
//            Ligand {
//                name:
//                    "N-{4-[(3-phenylpropyl)carbamoyl]phenyl}-2H-isoindole-2-carboxamide"
//                        .to_string(),
//                smiles: "c1ccc(cc1)CCCNC(=O)c2ccc(cc2)NC(=O)n3cc4ccccc4c3".to_string()
//            }
//        );

//        assert!(meta.solvents.is_some());
//        let solvents = meta.solvents.unwrap();
//        assert_eq!(solvents.len(), 1);

//        assert!(meta.papers.is_none());

//        assert!(meta.contributors.is_some());
//        let contributors = meta.contributors.unwrap();
//        assert_eq!(contributors.len(), 1);

//        assert!(meta.forcefield.is_some());
//        let forcefield = meta.forcefield.unwrap();
//        assert_eq!(forcefield.forcefield, Some("charmm36".to_string()));
//        assert_eq!(
//            forcefield.forcefield_comments,
//            Some("ligand parameters from swissparam".to_string())
//        );

//        assert!(meta.temperature.is_some());
//        let temperature = meta.temperature.unwrap();
//        assert_eq!(temperature.temperature, Some(300));

//        assert_eq!(meta.software.name, "GROMACS".to_string());
//        assert_eq!(meta.software.version, Some("2024".to_string()));

//        assert!(meta.water.is_some());
//        let water = meta.water.unwrap();
//        assert_eq!(water.is_present, true);
//        assert_eq!(water.model, Some("TIP3P".to_string()));
//        assert_eq!(water.density, Some(1000.));
//        assert_eq!(water.water_density_units, Some("g/m^3".to_string()));

//        //assert!(meta.required_files.is_some());
//        let required_files = meta.required_files;
//        assert_eq!(required_files.trajectory_file_name, "prodw.xtc".to_string());
//        assert_eq!(
//            required_files.structure_file_name,
//            "prod.part0135.pdb".to_string()
//        );
//        assert_eq!(required_files.topology_file_name, "prod.tpr".to_string());

//        assert!(meta.additional_files.is_some());
//        let additional_files = meta.additional_files.unwrap();
//        assert_eq!(additional_files.len(), 9);

//        Ok(())
//    }
//}
