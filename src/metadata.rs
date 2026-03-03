use crate::{common::read_file, constants};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow::Borrowed, path::PathBuf};
use validator::{Validate, ValidationError, ValidationErrorsKind};

#[derive(Debug, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub mdrepo_id: Option<String>,

    #[validate(regex(path = *constants::ORCID_REGEX))]
    pub lead_contributor_orcid: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub trajectory_file_name: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub structure_file_name: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub topology_file_name: String,

    #[validate(range(min = constants::TEMP_K_MIN, max = constants::TEMP_K_MAX))]
    pub temperature_kelvin: u32,

    #[validate(range(min = constants::TIMESTEP_FS_MIN, max = constants::TIMESTEP_FS_MAX))]
    pub integration_timestep_fs: u32,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub short_description: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub software_name: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub software_version: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[validate(
        range(
            min = constants::METADATA_TOML_VERSION,
            max = constants::METADATA_TOML_VERSION
            )
        )
    ]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toml_version: Option<u32>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_accession: Option<String>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_links: Option<Vec<ExternalLink>>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_commands: Option<String>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_files: Option<Vec<AdditionalFile>>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield: Option<String>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forcefield_comments: Option<String>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protonation_method: Option<String>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicate_id: Option<String>,

    #[validate(regex(path = *constants::PDB_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdb_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uniprot_ids: Option<Vec<String>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<Water>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ligands: Option<Vec<Ligand>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub solvents: Option<Vec<Solvent>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub papers: Option<Vec<Paper>>,

    #[validate(custom(function = "validate_dois"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dois: Option<Vec<String>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<Vec<Contributor>>,
}

impl Meta {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = read_file(path)?;
        toml::from_str(&contents)
            .map_err(|e| anyhow!(r#"Failed to parse "{}": {e}"#, path.display()))
    }

    pub fn from_string(contents: &str) -> Result<Self> {
        toml::from_str(&contents).map_err(|e| anyhow!(r#"Failed to parse input: {e}"#))
    }

    pub fn check(&self) -> Vec<String> {
        let mut messages = vec![];
        if let Err(e) = self.validate() {
            for (field, val) in e.errors() {
                for message in handle_validation_error_kind(field, val) {
                    messages.push(message);
                }
            }
        }
        messages
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct ExternalLink {
    #[validate(url)]
    pub url: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct AdditionalFile {
    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub file_type: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub file_name: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct Contributor {
    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub name: String,

    #[validate(regex(path = *constants::ORCID_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,

    #[validate(email)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Validate)]
pub struct Paper {
    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub title: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub authors: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub journal: String,

    #[validate(range(min = 0))]
    pub volume: u32,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,

    #[validate(range(min = 1900, max = 2030))]
    pub year: u32,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,

    #[validate(regex(path = *constants::DOI_REGEX))]
    pub doi: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate)]
pub struct Ligand {
    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub name: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub smiles: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct Solvent {
    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub name: String,

    #[validate(
       range(
           min = constants::SOLVENT_CONCENTRATION_MIN,
           max = constants::SOLVENT_CONCENTRATION_MAX
       )
    )]
    pub concentration_mol_liter: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct Water {
    #[validate(custom(function = "validate_water_model"))]
    pub model: String,

    #[validate(
       range(min = constants::WATER_DENSITY_MIN, max = constants::WATER_DENSITY_MAX)
    )]
    pub density_kg_m3: f32,
}

fn validate_dois(dois: &Vec<String>) -> Result<(), ValidationError> {
    if !dois.iter().all(|val| constants::DOI_REGEX.is_match(val)) {
        return Err(ValidationError::new("doi"));
    }
    Ok(())
}

fn validate_water_model(model: &str) -> Result<(), ValidationError> {
    if !constants::VALID_WATER_MODEL.contains(&model) {
        return Err(ValidationError::new("water_model"));
    }
    Ok(())
}

fn handle_validation_error_kind(
    field: &str,
    err_kind: &ValidationErrorsKind,
) -> Vec<String> {
    let mut messages = vec![];
    match err_kind {
        // Field(Vec<ValidationError>)
        ValidationErrorsKind::Field(errs) => {
            let message = errs
                .iter()
                .map(|e| format_validation_error(&e))
                .collect::<Vec<_>>()
                .join("; ");
            messages.push(format!("{field}: {message}"));
        }
        // List(BTreeMap<usize, Box<ValidationErrors>>)
        ValidationErrorsKind::List(tree) => {
            for (num, validation_errors) in tree {
                for (sub_fld, err_kind) in validation_errors.errors() {
                    let fld = format!("{field}[{}].{sub_fld}", num + 1);
                    for msg in handle_validation_error_kind(&fld, err_kind) {
                        messages.push(msg);
                    }
                }
            }
        }
        // Struct(Box<ValidationErrors>)
        ValidationErrorsKind::Struct(validation_errors) => {
            for (sub_fld, err_kind) in validation_errors.errors() {
                let fld = format!("{field}.{sub_fld}");
                for msg in handle_validation_error_kind(&fld, err_kind) {
                    messages.push(msg);
                }
            }
        }
    };
    messages
}

fn format_validation_error(err: &ValidationError) -> String {
    let given = match err.params.get("value") {
        Some(val) => serde_json::to_string(val).unwrap_or("".to_string()),
        _ => "".to_string(),
    };

    let message = match err.code {
        Borrowed("range") => {
            let min = err.params.get("min");
            let max = err.params.get("max");
            match (min, max) {
                (Some(x), None) => format!("must be >= {x}"),
                (None, Some(x)) => format!("must be <= {x}"),
                (Some(x), Some(y)) => {
                    if x == y {
                        format!("must be = {x}")
                    } else {
                        format!("must be >= {x} and <= {y}")
                    }
                }
                _ => "".to_string(),
            }
        }
        _ => "invalid".to_string(),
    };

    format!("value {given} {message}")
}

#[cfg(test)]
mod tests {
    const TOML_OK1: &str = "tests/inputs/toml/ok1.toml";
    const TOML_BAD1: &str = "tests/inputs/toml/bad1.toml";
    use super::Meta;
    use anyhow::Result;
    use std::path::PathBuf;
    use validator::Validate;

    #[test]
    fn meta_ok() -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(TOML_OK1))?;
        assert_eq!(meta.lead_contributor_orcid, "0000-0001-9961-144X");
        assert_eq!(meta.trajectory_file_name, "5aom.xtc");
        assert_eq!(meta.structure_file_name, "5aom_cleaned.pdb");
        assert_eq!(meta.topology_file_name, "5aom_gromacs_cleaned.top");
        assert_eq!(meta.temperature_kelvin, 300);
        assert_eq!(meta.integration_timestep_fs, 2);
        assert_eq!(meta.software_name, "AMBER");
        assert_eq!(
            meta.uniprot_ids,
            Some(vec!["P04637".to_string(), "Q9Y5S2".to_string()])
        );

        assert!(meta.water.is_some());
        if let Some(water) = &meta.water {
            assert_eq!(water.model, "TIP3P");
            assert_eq!(water.density_kg_m3, 1000.0);
        }

        assert!(meta.external_links.is_some());
        if let Some(links) = &meta.external_links {
            assert_eq!(links.len(), 1);
            if let Some(link) = links.first() {
                assert_eq!(link.url, "https://zenodo.org/records/7711953");
                assert_eq!(link.label, Some("Zenodo".to_string()));
            }
        }

        assert!(meta.ligands.is_some());
        if let Some(ligands) = &meta.ligands {
            assert_eq!(ligands.len(), 2);
            if let Some(ligand) = ligands.first() {
                assert_eq!(ligand.name, "FY8".to_string());
                assert_eq!(ligand.smiles, "Oc1ccc(Cl)cc1NC(=O)C2CCNCC2".to_string());
            }
        }

        assert!(meta.solvents.is_some());
        if let Some(solvents) = &meta.solvents {
            assert_eq!(solvents.len(), 2);
            if let Some(solvent) = solvents.first() {
                assert_eq!(solvent.name, "Na".to_string());
                assert_eq!(solvent.concentration_mol_liter, 0.15);
            }
        }

        assert!(meta.contributors.is_some());
        if let Some(contributors) = &meta.contributors {
            assert_eq!(contributors.len(), 1);
            if let Some(c) = contributors.first() {
                assert_eq!(c.name, "Alex Leifson".to_string());
                assert_eq!(c.orcid, Some("0000-0003-2819-749X".to_string()));
                assert_eq!(c.email, Some("alex@aol.com".to_string()));
                assert_eq!(c.institution, Some("University of Montreal".to_string()));
            }
        }

        assert!(meta.uniprot_ids.is_some());
        if let Some(uniprot_ids) = &meta.uniprot_ids {
            assert_eq!(uniprot_ids.len(), 2);
            assert_eq!(
                uniprot_ids,
                &vec!["P04637".to_string(), "Q9Y5S2".to_string()]
            );
        }

        assert!(meta.dois.is_some());
        if let Some(dois) = &meta.dois {
            assert_eq!(dois.len(), 1);
            if let Some(doi) = dois.first() {
                assert_eq!(doi, "10.1038/s43588-024-00627-2");
            }
        }

        assert!(meta.papers.is_some());
        if let Some(papers) = &meta.papers {
            assert_eq!(papers.len(), 1);
            if let Some(paper) = papers.first() {
                assert_eq!(
                    paper.title,
                    "MISATO: machine learning dataset of protein–ligand complexes \
                    for structure-based drug discovery"
                        .to_string()
                );
            }
        }

        assert!(meta.additional_files.is_some());
        if let Some(files) = &meta.additional_files {
            assert_eq!(files.len(), 1);
            if let Some(file) = files.first() {
                assert_eq!(file.file_type, "Structure".to_string());
                assert_eq!(file.file_name, "5aom_gromacs_cleaned.gro".to_string());
                assert_eq!(
                    file.description,
                    Some(".gro format of the structure file".to_string())
                );
            }
        }

        assert!(&meta.validate().is_ok());

        Ok(())
    }

    #[test]
    fn meta_validate_bad() -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(TOML_BAD1))?;
        let errors = &meta.validate();
        assert!(errors.is_err());
        Ok(())
    }

    #[test]
    fn meta_check_bad() -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(TOML_BAD1))?;
        let errors = &meta.check();
        let expected = vec![
            r#"contributors[1].orcid: value "0000-2819-749X" invalid"#,
            r#"contributors[1].email: value "alex" invalid"#,
            r#"additional_files[1].file_type: value " " invalid"#,
            r#"additional_files[1].file_name: value " " invalid"#,
            r#"additional_files[1].description: value " " invalid"#,
            r#"topology_file_name: value " " invalid"#,
            r#"short_description: value " " invalid"#,
            r#"temperature_kelvin: value 0 must be >= 275 and <= 700"#,
            r#"dois: value ["1038/s43588-024-00627-2"] invalid"#,
            r#"trajectory_file_name: value " " invalid"#,
            r#"external_links[1].url: value "zenodo.org/records/7711953" invalid"#,
            r#"external_links[1].label: value " " invalid"#,
            r#"forcefield: value " " invalid"#,
            r#"pdb_id: value "5am" invalid"#,
            r#"water.model: value "XYZ" invalid"#,
            r#"water.density_kg_m3: value 1000000.0 must be >= 900.0 and <= 1100.0"#,
            r#"lead_contributor_orcid: value "0000-0001-9961-144" invalid"#,
            r#"structure_file_name: value " " invalid"#,
            r#"integration_timestep_fs: value 2000 must be >= 1 and <= 5"#,
            r#"toml_version: value 4 must be = 2"#,
        ];
        assert_eq!(errors.len(), expected.len());
        for message in expected {
            assert!(errors.contains(&message.to_string()));
        }
        Ok(())
    }
}
