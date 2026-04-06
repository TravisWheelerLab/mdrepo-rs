use crate::{common::read_file, constants};
use anyhow::{anyhow, bail, Result};
use multiset::HashMultiSet;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow::Borrowed,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use validator::{Validate, ValidationError, ValidationErrorsKind};

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct MetaCheckOptions {
    pub allow_no_pdb_uniprot: bool,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Meta {
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

    #[validate(
        range(min = constants::TIMESTEP_FS_MIN, max = constants::TIMESTEP_FS_MAX)
    )]
    pub integration_timestep_fs: u32,

    #[validate(length(max = 300), regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub short_description: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub software_name: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub software_version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdrepo_id: Option<String>,

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

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "commands")]
    pub run_commands: Option<String>,

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
    pub additional_files: Option<Vec<AdditionalFile>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ligands: Option<Vec<Ligand>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solvents: Option<Vec<Solvent>>,

    #[validate(custom(function = "validate_dois"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dois: Option<Vec<String>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub papers: Option<Vec<Paper>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_links: Option<Vec<ExternalLink>>,

    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<Vec<Contributor>>,
}

impl Meta {
    pub fn all_filenames(&self) -> Vec<String> {
        let mut filenames = vec![
            self.trajectory_file_name.clone(),
            self.structure_file_name.clone(),
            self.topology_file_name.clone(),
        ];

        if let Some(files) = &self.additional_files {
            for file in files {
                filenames.push(file.file_name.clone());
            }
        }

        filenames
    }

    pub fn check(&self, options: Option<MetaCheckOptions>) -> Vec<String> {
        let mut messages = vec![];
        if let Err(e) = self.validate() {
            for (field, val) in e.errors() {
                for message in handle_validation_error_kind(field, val) {
                    messages.push(message);
                }
            }
        }

        // Check file extensions of required files
        let ext_checks = [
            (
                "trajectory_file_name",
                &self.trajectory_file_name,
                constants::TRAJECTORY_FILE_EXTS,
            ),
            (
                "structure_file_name",
                &self.structure_file_name,
                constants::STRUCTURE_FILE_EXTS,
            ),
            (
                "topology_file_name",
                &self.topology_file_name,
                constants::TOPOLOGY_FILE_EXTS,
            ),
        ];

        for (field, filename, valid_exts) in ext_checks {
            if let Some(ext) = Path::new(filename).extension() {
                let ext = ext.to_string_lossy().to_string();
                if !valid_exts.contains(&ext.as_str()) {
                    messages.push(format!(
                        r#"{field}: Invalid extension "{ext}"; choose from {}"#,
                        valid_exts.join(", ")
                    ))
                }
            } else {
                messages.push(format!(r#"{field}: Filename is missing extension"#))
            }
        }

        // All the messages up to this point start with "field_name: "
        // , so sort to put all the field errors together.
        messages.sort();

        // Ensure that each filename is present only once
        let mut file_count = HashMultiSet::new();
        file_count.insert(self.trajectory_file_name.clone());
        file_count.insert(self.topology_file_name.clone());
        file_count.insert(self.structure_file_name.clone());
        if let Some(addl_files) = &self.additional_files {
            for file in addl_files {
                file_count.insert(file.file_name.clone());
            }
        }

        for filename in file_count.distinct_elements() {
            let count = file_count.count_of(filename);
            if count > 1 {
                messages.push(format!(
                    r#"Filename "{filename}" is duplicated {count} times"#
                ));
            }
        }

        // Special check for GROMACS with only a ".top" file
        let is_gromacs = self.software_name.to_lowercase().contains("gromacs");
        if is_gromacs
            && Path::new(&self.topology_file_name).extension()
                == Some(OsStr::new("top"))
        {
            let exts: Vec<String> = match &self.additional_files {
                Some(files) => files
                    .iter()
                    .filter_map(|f| {
                        Path::new(&f.file_name)
                            .extension()
                            .map(|e| e.to_string_lossy().to_string())
                    })
                    .collect(),
                _ => vec![],
            };

            if !&["tpr", "gro"]
                .iter()
                .any(|ext| exts.contains(&ext.to_string()))
            {
                messages.push(
                    "topology_file_name: GROMACS topology \".top\" file requires \
                    additional \".tpr\" or \".gro\""
                        .to_string(),
                );
            }
        }

        if self.pdb_id.is_none()
            && self.uniprot_ids.is_none()
            && !options.map_or(false, |val| val.allow_no_pdb_uniprot)
        {
            messages
                .push("Missing PDB and Uniprot IDs (skip with --no-id)".to_string());
        }

        messages
    }

    pub fn from_file(path: &PathBuf) -> Result<Self> {
        match path.extension() {
            Some(ext) => {
                let contents = read_file(path)?;
                if contents.is_empty() {
                    bail!("File is empty")
                }
                let meta = match ext.to_str() {
                    Some("json") => Self::from_json(&contents)?,
                    Some("toml") => Self::from_toml(&contents)?,
                    _ => bail!(r#"Unknown file extension "{}""#, ext.display()),
                };
                Ok(meta)
            }
            _ => bail!("No file extension"),
        }
    }

    pub fn from_toml(contents: &str) -> Result<Self> {
        toml::from_str(contents).map_err(|e| anyhow!(r#"Failed to parse input: {e}"#))
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| anyhow!(r#"Failed to parse input: {e}"#))
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(&self).map_err(Into::into)
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self).map_err(Into::into)
    }

    pub fn example() -> Self {
        Meta {
            mdrepo_id: None,
            lead_contributor_orcid: "0000-0000-0000-0000".to_string(),
            trajectory_file_name: "traj.xtc".to_string(),
            structure_file_name: "struct.pdb".to_string(),
            topology_file_name: "topology.gro".to_string(),
            temperature_kelvin: 300,
            integration_timestep_fs: 2,
            short_description: "<short_description> (required)".to_string(),
            description: Some("<longer description>".to_string()),
            software_name: "GROMACS".to_string(),
            software_version: "2024.5".to_string(),
            toml_version: Some(2),
            user_accession: Some("ABC123".to_string()),
            external_links: Some(vec![ExternalLink {
                url: "http://aol.com".to_string(),
                label: Some("My Link".to_string()),
            }]),
            run_commands: Some("gmx_mpi mdrun".to_string()),
            additional_files: None,
            forcefield: Some("CHARMM36m".to_string()),
            forcefield_comments: Some(
                "ligand params: CGenFF and SwissParam".to_string(),
            ),
            protonation_method: Some("PropKa".to_string()),
            replicate_id: Some("MyReplicateGroupABC".to_string()),
            pdb_id: Some("5emo".to_string()),
            uniprot_ids: Some(vec!["A0A0H2UWN8".to_string(), "S8G8I1".to_string()]),
            water: Some(Water {
                model: "TIP3P".to_string(),
                density_kg_m3: 986.,
            }),
            ligands: Some(vec![Ligand {
                name: "Foropafant".to_string(),
                smiles:
                    "CC(C)C1=CC(=C(C(=C1)C(C)C)C2=CSC(=N2)N(CCN(C)C)CC3=CN=CC=C3)C(C)C"
                        .to_string(),
            }]),
            solvents: Some(vec![Solvent {
                name: "Na".to_string(),
                concentration_mol_liter: 0.15,
            }]),
            papers: None,
            dois: Some(vec!["10.1017/j.str.2019.08.032".to_string()]),
            contributors: Some(vec![Contributor {
                name: "Barbara McClintock".to_string(),
                institution: Some("Cold Spring Harbor Laboratory".to_string()),
                email: Some("barb@cshl.edu".to_string()),
                orcid: Some("0000-0002-6897-9608".to_string()),
            }]),
        }
    }

    pub fn example_minimal() -> Self {
        Meta {
            mdrepo_id: None,
            lead_contributor_orcid: "0000-0000-0000-0000".to_string(),
            trajectory_file_name: "traj.xtc".to_string(),
            structure_file_name: "struct.pdb".to_string(),
            topology_file_name: "topology.gro".to_string(),
            temperature_kelvin: 300,
            integration_timestep_fs: 2,
            short_description: "<short_description> (required)".to_string(),
            description: None,
            software_name: "GROMACS".to_string(),
            software_version: "2024.5".to_string(),
            toml_version: None,
            user_accession: None,
            external_links: None,
            run_commands: None,
            additional_files: None,
            forcefield: None,
            forcefield_comments: None,
            protonation_method: None,
            replicate_id: None,
            pdb_id: None,
            uniprot_ids: None,
            water: None,
            ligands: None,
            solvents: None,
            papers: None,
            dois: None,
            contributors: None,
        }
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
    pub file_name: String,

    #[validate(length(max = 32), regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub file_type: String,

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

    #[validate(custom(function = "is_valid_smiles"))]
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
    pub density_kg_m3: f64,
}

fn validate_dois(dois: &[String]) -> Result<(), ValidationError> {
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
                .map(format_validation_error)
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

// --------------------------------------------------
fn format_validation_error(err: &ValidationError) -> String {
    let given = match err.params.get("value") {
        Some(val) => serde_json::to_string(val).unwrap_or("".to_string()),
        _ => "".to_string(),
    };

    let message = match err.code {
        Borrowed("length") => {
            let min = err.params.get("min");
            let max = err.params.get("max");
            match (min, max) {
                (Some(x), None) => format!("length must be >= {x}"),
                (None, Some(x)) => format!("length must be <= {x}"),
                (Some(x), Some(y)) => {
                    if x == y {
                        format!("length must be = {x}")
                    } else {
                        format!("length must be >= {x} and <= {y}")
                    }
                }
                _ => "".to_string(),
            }
        }
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

// --------------------------------------------------
pub fn is_valid_smiles(smiles: &str) -> std::result::Result<(), ValidationError> {
    let mut writer = purr::write::Writer::new();
    let mut trace = purr::read::Trace::new();
    if purr::read::read(smiles, &mut writer, Some(&mut trace)).is_ok() {
        Ok(())
    } else {
        Err(ValidationError::new("invalid SMILES"))
    }
}

// --------------------------------------------------
#[cfg(test)]
mod proptest_tests {
    use super::*;
    use crate::constants;
    use proptest::prelude::*;
    use validator::Validate;

    fn base_meta() -> Meta {
        Meta::example_minimal()
    }

    proptest! {
        // --- Regex: ORCID ---

        #[test]
        fn valid_orcid_matches_regex(
            a in "[0-9]{4}",
            b in "[0-9]{4}",
            c in "[0-9]{4}",
            d in "[A-Z0-9]{4}",
        ) {
            let orcid = format!("{a}-{b}-{c}-{d}");
            prop_assert!(constants::ORCID_REGEX.is_match(&orcid));
        }

        // --- Regex: NOT_WHITESPACE ---

        #[test]
        fn whitespace_only_fails_not_whitespace(s in "[ \t\n\r]+") {
            prop_assert!(!constants::NOT_WHITESPACE_REGEX.is_match(&s));
        }

        #[test]
        fn non_whitespace_string_passes(s in "[a-zA-Z0-9][a-zA-Z0-9 ]{0,50}") {
            prop_assert!(constants::NOT_WHITESPACE_REGEX.is_match(&s));
        }

        // --- Regex: PDB ID ---

        #[test]
        fn valid_pdb_id_matches_regex(s in "[A-Za-z0-9]{4}") {
            prop_assert!(constants::PDB_REGEX.is_match(&s));
        }

        #[test]
        fn pdb_id_wrong_length_fails_regex(
            s in "[A-Za-z0-9]{1,3}|[A-Za-z0-9]{5,10}",
        ) {
            prop_assert!(!constants::PDB_REGEX.is_match(&s));
        }

        // --- Regex: DOI ---

        #[test]
        fn valid_doi_matches_regex(
            use_url_prefix in any::<bool>(),
            digits in "[0-9]{4,5}",
            suffix in "[a-zA-Z0-9-]{1,15}[a-zA-Z0-9]",
        ) {
            let doi = if use_url_prefix {
                format!("https://doi.org/10.{digits}/{suffix}")
            } else {
                format!("10.{digits}/{suffix}")
            };
            prop_assert!(
                constants::DOI_REGEX.is_match(&doi),
                "DOI '{}' should match", doi
            );
        }

        // --- Range: temperature_kelvin ---

        #[test]
        fn valid_temperature_no_error(temp in 275u32..=700u32) {
            let mut meta = base_meta();
            meta.temperature_kelvin = temp;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("temperature_kelvin:"));
            prop_assert!(!has_error, "Unexpected temperature error for {temp}: {errors:?}");
        }

        #[test]
        fn out_of_range_temperature_produces_error(temp in prop_oneof![
            0u32..275u32,
            701u32..=u32::MAX,
        ]) {
            let mut meta = base_meta();
            meta.temperature_kelvin = temp;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("temperature_kelvin:"));
            prop_assert!(has_error, "Expected temperature error for temp={temp}");
        }

        // --- Range: integration_timestep_fs ---

        #[test]
        fn valid_timestep_no_error(timestep in 1u32..=5u32) {
            let mut meta = base_meta();
            meta.integration_timestep_fs = timestep;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("integration_timestep_fs:"));
            prop_assert!(!has_error, "Unexpected timestep error for {timestep}: {errors:?}");
        }

        #[test]
        fn out_of_range_timestep_produces_error(timestep in prop_oneof![
            0u32..1u32,
            6u32..=u32::MAX,
        ]) {
            let mut meta = base_meta();
            meta.integration_timestep_fs = timestep;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("integration_timestep_fs:"));
            prop_assert!(has_error, "Expected timestep error for timestep={timestep}");
        }

        // --- Range: Water density ---

        #[test]
        fn valid_water_density_passes(density in 900.0f64..=1100.0f64) {
            let water = Water {
                model: "TIP3P".to_string(),
                density_kg_m3: density,
            };
            prop_assert!(water.validate().is_ok());
        }

        #[test]
        fn out_of_range_water_density_fails(density in prop_oneof![
            0.1f64..900.0f64,
            1100.001f64..2000.0f64,
        ]) {
            let water = Water {
                model: "TIP3P".to_string(),
                density_kg_m3: density,
            };
            prop_assert!(water.validate().is_err());
        }

        // --- Range: Solvent concentration ---

        #[test]
        fn valid_solvent_concentration_passes(conc in 0.0f64..=1.0f64) {
            let solvent = Solvent {
                name: "Na".to_string(),
                concentration_mol_liter: conc,
            };
            prop_assert!(solvent.validate().is_ok());
        }

        #[test]
        fn out_of_range_solvent_concentration_fails(conc in prop_oneof![
            -10.0f64..-0.001f64,
            1.001f64..10.0f64,
        ]) {
            let solvent = Solvent {
                name: "Na".to_string(),
                concentration_mol_liter: conc,
            };
            prop_assert!(solvent.validate().is_err());
        }

        // --- Range: Paper year ---

        #[test]
        fn valid_paper_year_passes(year in 1900u32..=2030u32) {
            let paper = Paper {
                title: "Title".to_string(),
                authors: "Author".to_string(),
                journal: "Journal".to_string(),
                volume: 1,
                number: None,
                year,
                pages: None,
                doi: None,
            };
            prop_assert!(paper.validate().is_ok(), "Year {year} should be valid");
        }

        #[test]
        fn out_of_range_paper_year_fails(year in prop_oneof![
            0u32..1900u32,
            2031u32..=u32::MAX,
        ]) {
            let paper = Paper {
                title: "Title".to_string(),
                authors: "Author".to_string(),
                journal: "Journal".to_string(),
                volume: 1,
                number: None,
                year,
                pages: None,
                doi: None,
            };
            prop_assert!(paper.validate().is_err(), "Year {year} should be invalid");
        }

        // --- short_description length ---

        #[test]
        fn short_description_within_300_passes(
            desc in "[a-zA-Z0-9][a-zA-Z0-9 ]{0,299}",
        ) {
            let mut meta = base_meta();
            meta.short_description = desc;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("short_description:"));
            prop_assert!(!has_error, "Unexpected description error: {errors:?}");
        }

        #[test]
        fn short_description_over_300_fails(extra in "[a-zA-Z0-9]{1,100}") {
            let mut meta = base_meta();
            meta.short_description = format!("{}{}", "a".repeat(300), extra);
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("short_description:"));
            prop_assert!(has_error, "Expected description length error, got: {errors:?}");
        }

        // --- Serialization round-trips ---

        #[test]
        fn meta_toml_round_trip(
            temp in 275u32..=700u32,
            timestep in 1u32..=5u32,
        ) {
            let mut meta = base_meta();
            meta.temperature_kelvin = temp;
            meta.integration_timestep_fs = timestep;
            let toml_str = meta.to_toml().expect("serialization failed");
            let meta2 = Meta::from_toml(&toml_str).expect("deserialization failed");
            prop_assert_eq!(meta2.temperature_kelvin, temp);
            prop_assert_eq!(meta2.integration_timestep_fs, timestep);
        }

        #[test]
        fn meta_json_round_trip(
            temp in 275u32..=700u32,
            timestep in 1u32..=5u32,
        ) {
            let mut meta = base_meta();
            meta.temperature_kelvin = temp;
            meta.integration_timestep_fs = timestep;
            let json_str = meta.to_json().expect("serialization failed");
            let meta2 = Meta::from_json(&json_str).expect("deserialization failed");
            prop_assert_eq!(meta2.temperature_kelvin, temp);
            prop_assert_eq!(meta2.integration_timestep_fs, timestep);
        }

        // --- Duplicate filename detection ---

        #[test]
        fn duplicate_filename_detected(name in "[a-z]{2,8}") {
            let mut meta = base_meta();
            let filename = format!("{name}.xtc");
            // Same filename in both trajectory and structure fields → duplicate
            meta.trajectory_file_name = filename.clone();
            meta.structure_file_name = filename.clone();
            let errors = meta.check(None);
            let has_dup_error = errors.iter().any(|e| e.contains("is duplicated"));
            prop_assert!(
                has_dup_error,
                "Expected duplicate error for '{filename}', got: {errors:?}"
            );
        }
    }

    // --- GROMACS .top special rule (non-proptest) ---

    #[test]
    fn gromacs_top_without_tpr_or_gro_fails() {
        let mut meta = base_meta();
        meta.software_name = "GROMACS".to_string();
        meta.topology_file_name = "topol.top".to_string();
        meta.additional_files = None;
        let errors = meta.check(None);
        assert!(
            errors.iter().any(|e| e.contains("GROMACS topology")),
            "Expected GROMACS topology error, got: {errors:?}"
        );
    }

    #[test]
    fn gromacs_top_with_tpr_passes_gromacs_check() {
        let mut meta = base_meta();
        meta.software_name = "GROMACS".to_string();
        meta.topology_file_name = "topol.top".to_string();
        meta.additional_files = Some(vec![AdditionalFile {
            file_name: "run.tpr".to_string(),
            file_type: "Binary topology".to_string(),
            description: None,
        }]);
        let errors = meta.check(None);
        assert!(
            !errors.iter().any(|e| e.contains("GROMACS topology")),
            "Expected no GROMACS topology error, got: {errors:?}"
        );
    }

    #[test]
    fn gromacs_top_with_gro_passes_gromacs_check() {
        let mut meta = base_meta();
        meta.software_name = "GROMACS".to_string();
        meta.topology_file_name = "topol.top".to_string();
        meta.additional_files = Some(vec![AdditionalFile {
            file_name: "struct.gro".to_string(),
            file_type: "Structure".to_string(),
            description: None,
        }]);
        let errors = meta.check(None);
        assert!(
            !errors.iter().any(|e| e.contains("GROMACS topology")),
            "Expected no GROMACS topology error, got: {errors:?}"
        );
    }
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    const TOML_OK1: &str = "tests/inputs/metadata/ok1.toml";
    const TOML_BAD1: &str = "tests/inputs/metadata/bad1.toml";
    const JSON_OK1: &str = "tests/inputs/metadata/ok1.json";
    const JSON_BAD1: &str = "tests/inputs/metadata/bad1.json";

    use super::Meta;
    use anyhow::Result;
    use std::path::PathBuf;
    use validator::Validate;

    #[test]
    fn meta_toml_ok() -> Result<()> {
        meta_ok(TOML_OK1)
    }

    #[test]
    fn meta_json_ok() -> Result<()> {
        meta_ok(JSON_OK1)
    }

    fn meta_ok(filename: &str) -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(filename))?;
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
    fn meta_validate_toml_bad() -> Result<()> {
        meta_validate_bad(TOML_BAD1)
    }

    #[test]
    fn meta_validate_json_bad() -> Result<()> {
        meta_validate_bad(JSON_BAD1)
    }

    fn meta_validate_bad(filename: &str) -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(filename))?;
        let errors = &meta.validate();
        assert!(errors.is_err());
        Ok(())
    }

    #[test]
    fn meta_check_bad() -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(TOML_BAD1))?;
        let errors = &meta.check(None);
        let expected = vec![
            r#"additional_files[1].description: value " " invalid"#,
            r#"additional_files[1].file_name: value " " invalid"#,
            r#"additional_files[1].file_type: value " " invalid"#,
            r#"contributors[1].email: value "alex" invalid"#,
            r#"contributors[1].orcid: value "0000-2819-749X" invalid"#,
            r#"dois: value ["1038/s43588-024-00627-2"] invalid"#,
            r#"external_links[1].label: value " " invalid"#,
            r#"external_links[1].url: value "zenodo.org/records/7711953" invalid"#,
            r#"forcefield: value " " invalid"#,
            r#"integration_timestep_fs: value 2000 must be >= 1 and <= 5"#,
            r#"lead_contributor_orcid: value "0000-0001-9961-144" invalid"#,
            r#"ligands[1].smiles: value "smiles_string" invalid"#,
            r#"pdb_id: value "5am" invalid"#,
            r#"short_description: value " " invalid"#,
            r#"solvents[1].name: value " " invalid"#,
            r#"structure_file_name: Filename is missing extension"#,
            r#"structure_file_name: value " " invalid"#,
            r#"temperature_kelvin: value 0 must be >= 275 and <= 700"#,
            r#"toml_version: value 4 must be = 2"#,
            r#"topology_file_name: Filename is missing extension"#,
            r#"topology_file_name: value " " invalid"#,
            r#"trajectory_file_name: Filename is missing extension"#,
            r#"trajectory_file_name: value " " invalid"#,
            r#"water.density_kg_m3: value 1000000.0 must be >= 900.0 and <= 1100.0"#,
            r#"water.model: value "XYZ" invalid"#,
            r#"Filename " " is duplicated 4 times"#,
        ];
        assert_eq!(errors.len(), expected.len());
        for message in expected {
            assert!(errors.contains(&message.to_string()));
        }
        Ok(())
    }
}
