use crate::{common::read_file, constants};
use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow::Borrowed, collections::HashMap, ffi::OsStr, path::Path};
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

    #[validate(length(min = 1))]
    pub trajectory_file_names: Vec<String>,

    pub structure_file_name: String,

    pub topology_file_name: String,

    #[validate(range(min = constants::TEMP_K_MIN, max = constants::TEMP_K_MAX))]
    pub temperature_kelvin: u32,

    #[validate(
        range(min = constants::TIMESTEP_FS_MIN, max = constants::TIMESTEP_FS_MAX)
    )]
    pub integration_timestep_fs: u32,

    /// Simulated time between consecutive saved frames, in picoseconds --
    /// the output frequency (AMBER's `ntwx`, GROMACS's `nstxout`) times the
    /// integration timestep. Optional, and read on two occasions.
    ///
    /// It began as a fallback for the trajectories that cannot state their own
    /// spacing -- an AMBER NetCDF written without a `time` variable, or an
    /// ASCII mdcrd, which stores no timing at all. Absent both, conversion to
    /// XTC stamps cpptraj's default 1 ps/frame onto the output and every
    /// duration and sampling frequency downstream inherits that silently.
    /// There the declaration substitutes for a measurement that does not
    /// exist, and nothing can check it.
    ///
    /// Since 2026-09-08 it is also read when a spacing WAS measured and comes
    /// out below `SAMPLING_FLOOR_PS`. There it corroborates rather than
    /// substitutes: it is required, and it must agree with what the trajectory
    /// reports. So "a trajectory with its own time axis is ground truth and
    /// this is ignored" holds at or above the floor, and below it the two have
    /// to say the same thing.
    #[validate(range(
        min = constants::SAMPLING_FREQUENCY_PS_MIN,
        max = constants::SAMPLING_FREQUENCY_PS_MAX
    ))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling_frequency_ps: Option<f64>,

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
    pub alias: Option<String>,

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

    #[validate(regex(path = *constants::PDB_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdb_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(custom(function = "validate_uniprot_ids"))]
    pub uniprot_ids: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(custom(function = "validate_collections"))]
    pub collections: Option<Vec<String>>,

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
    pub solutes: Option<Vec<Solute>>,

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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_embargoed: Option<bool>,
}

struct FormatCombination {
    structure_ext: String,
    topology_ext: String,
    trajectory_exts: Vec<String>,
    matched_engines: Vec<&'static str>,
}

impl FormatCombination {
    fn trajectory_display(&self) -> String {
        format!(".{}", self.trajectory_exts.join(", ."))
    }
}

impl Meta {
    pub fn all_filenames(&self) -> Vec<String> {
        let mut filenames = vec![
            self.structure_file_name.clone(),
            self.topology_file_name.clone(),
        ];

        filenames.extend_from_slice(&self.trajectory_file_names);

        if let Some(files) = &self.additional_files {
            for file in files {
                filenames.push(file.file_name.clone());
            }
        }

        filenames
    }

    // --------------------------------------------------
    pub fn check(&self, options: Option<MetaCheckOptions>) -> Vec<String> {
        let mut messages = vec![];
        if let Err(e) = self.validate() {
            for (field, val) in e.errors() {
                messages.extend(handle_validation_error_kind(field, val));
            }
        }

        let software_name = self.software_name.as_str();
        if let Some(valid_versions) = constants::VALID_SOFTWARE.get(software_name) {
            if !valid_versions.iter().any(|&v| v == self.software_version) {
                messages.push(format!(
                    r#"software_version: "{}" invalid for "{software_name}", choose from {}"#,
                    &self.software_version,
                    valid_versions.join(", ")
                ))
            }
        } else {
            let valid_software_names: Vec<String> = constants::VALID_SOFTWARE
                .keys()
                .map(|val| val.to_string())
                .collect();
            messages.push(format!(
                r#"software_name: "{}" invalid, choose from {}"#,
                self.software_name,
                valid_software_names.join(", "),
            ))
        }

        // A frame cannot be saved more often than the integrator steps, so a
        // declared spacing shorter than one timestep is a mistake -- most
        // likely a value entered in the wrong unit. Both fields must have
        // passed their own range checks first, so one bad number is reported
        // once rather than twice.
        //
        // Subsumed as of 2026-09-08 and kept anyway: SAMPLING_FREQUENCY_PS_MIN
        // is now 1 ps and TIMESTEP_FS_MAX is 20 fs (0.02 ps), so a value that
        // passes its own range check can no longer be shorter than a timestep
        // and this can no longer fire. It stays because it guards a real
        // physical fact rather than the current constants, and it becomes live
        // again the moment either bound moves.
        if let Some(sampling_ps) = self.sampling_frequency_ps
            && (constants::SAMPLING_FREQUENCY_PS_MIN
                ..=constants::SAMPLING_FREQUENCY_PS_MAX)
                .contains(&sampling_ps)
            && (constants::TIMESTEP_FS_MIN..=constants::TIMESTEP_FS_MAX)
                .contains(&self.integration_timestep_fs)
        {
            let timestep_ps = f64::from(self.integration_timestep_fs) / 1000.;
            if sampling_ps < timestep_ps {
                messages.push(format!(
                    "sampling_frequency_ps: {sampling_ps} ps is shorter than \
                    the {} fs integration timestep -- a frame cannot be saved \
                    more often than every step",
                    self.integration_timestep_fs
                ));
            }
        }

        // Each of structure/topology/trajectory needs *an* extension to be
        // checkable at all -- that much is field-independent. Whether it is
        // the *right* extension is not: that depends on the other two
        // fields and is decided below, as a set, against
        // constants::ENGINE_FILE_FORMATS, not against a flat per-field
        // allowlist. A union-of-all-engines allowlist per field is exactly
        // how ticket 1997 slipped through: individually "valid" extensions
        // that no single engine actually produces together.
        let name_checks = [
            ("structure_file_name", &self.structure_file_name),
            ("topology_file_name", &self.topology_file_name),
        ];

        // Ensure that each filename is present only once
        let mut file_count: HashMap<String, usize> = HashMap::new();
        for (fieldname, filename) in name_checks {
            if let Some(msg) = Self::check_has_extension(fieldname, filename) {
                messages.push(msg);
            }

            if let Some(msg) = Self::check_no_wildcard(fieldname, filename) {
                messages.push(msg);
            }

            file_count
                .entry(filename.to_string())
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }

        for (i, filename) in self.trajectory_file_names.iter().enumerate() {
            if let Some(msg) = Self::check_has_extension(
                &format!("trajectory_file_names[{}]", i + 1),
                filename,
            ) {
                messages.push(msg);
            }

            if let Some(msg) = Self::check_no_wildcard(
                &format!("trajectory_file_names[{}]", i + 1),
                filename,
            ) {
                messages.push(msg);
            }

            file_count
                .entry(filename.to_string())
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }

        // All the messages up to this point start with "field_name: ",
        // so sort to put all the field errors together.
        messages.sort();

        if let Some(addl_files) = &self.additional_files {
            for (i, file) in addl_files.iter().enumerate() {
                if let Some(msg) = Self::check_no_wildcard(
                    &format!("additional_files[{}].file_name", i + 1),
                    &file.file_name,
                ) {
                    messages.push(msg);
                }

                file_count
                    .entry(file.file_name.to_string())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
        }

        for (filename, count) in file_count {
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

            if !exts.iter().any(|e| matches!(e.as_str(), "tpr" | "gro")) {
                messages.push(
                    "topology_file_name: GROMACS topology \".top\" file requires \
                    additional \".tpr\" or \".gro\""
                        .to_string(),
                );
            }
        }

        // Check that structure/topology/trajectory extensions form a
        // combination the declared engine could actually have produced.
        // CUSTOM is the deliberate escape hatch for pipelines with no fixed
        // convention, so it is exempt. Two distinct failures, both fatal:
        // the combination matches no engine at all (incompatible mixing),
        // or it matches some *other* engine but not the declared one
        // (software_name does not describe what was actually submitted).
        if self.software_name != "CUSTOM"
            && let Some(combo) = self.format_combination()
        {
            if combo.matched_engines.is_empty() {
                messages.push(format!(
                    "File formats are incompatible: structure \".{}\", \
                    topology \".{}\", trajectory {} match no supported \
                    engine's convention",
                    combo.structure_ext,
                    combo.topology_ext,
                    combo.trajectory_display(),
                ));
            } else if !combo.matched_engines.contains(&self.software_name.as_str()) {
                messages.push(format!(
                    "software_name is \"{}\" but structure/topology/\
                    trajectory extensions (.{}, .{}, {}) match {} instead",
                    self.software_name,
                    combo.structure_ext,
                    combo.topology_ext,
                    combo.trajectory_display(),
                    combo.matched_engines.join(" or "),
                ));
            }
        }

        if self.pdb_id.is_none()
            && self.uniprot_ids.is_none()
            && !options.is_some_and(|val| val.allow_no_pdb_uniprot)
        {
            messages
                .push("Missing PDB and Uniprot IDs (skip with --no-id)".to_string());
        }

        messages
    }

    /// Extracts the declared structure/topology/trajectory extensions and
    /// finds which engines in `constants::ENGINE_FILE_FORMATS` accept all
    /// three at once. Returns None when a filename is missing an extension
    /// entirely -- that is already reported by `check_file_extensions`, so
    /// there is nothing useful to add here.
    fn format_combination(&self) -> Option<FormatCombination> {
        let ext_of = |filename: &str| -> Option<String> {
            Path::new(filename)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
        };

        let structure_ext = ext_of(&self.structure_file_name)?;
        let topology_ext = ext_of(&self.topology_file_name)?;
        if self.trajectory_file_names.is_empty() {
            return None;
        }
        let trajectory_exts: Vec<String> = self
            .trajectory_file_names
            .iter()
            .filter_map(|f| ext_of(f))
            .collect();
        if trajectory_exts.len() != self.trajectory_file_names.len() {
            return None;
        }

        let matched_engines: Vec<&'static str> = constants::ENGINE_FILE_FORMATS
            .iter()
            .filter(|(_, fmt)| {
                fmt.structure.contains(&structure_ext.as_str())
                    && fmt.topology.contains(&topology_ext.as_str())
                    && trajectory_exts
                        .iter()
                        .all(|ext| fmt.trajectory.contains(&ext.as_str()))
            })
            .map(|(&name, _)| name)
            .collect();

        Some(FormatCombination {
            structure_ext,
            topology_ext,
            trajectory_exts,
            matched_engines,
        })
    }

    /// Only checks that `filename` has an extension at all -- whether it is
    /// the *right* one is the combination check's job (see
    /// `format_combination`), since that depends jointly on all three of
    /// structure/topology/trajectory, not on this field alone.
    fn check_has_extension(fieldname: &str, filename: &str) -> Option<String> {
        if Path::new(filename).extension().is_some() {
            None
        } else {
            Some(format!(
                r#"{fieldname}: filename "{filename}" is missing extension"#
            ))
        }
    }

    /// A wildcard is not a filename. NOTHING in this codebase expands globs
    /// -- `validate::validate_meta` stats every name from `all_filenames()`
    /// literally -- so a pattern that reaches that point fails as
    /// `Missing file "Pro_lig*.mdc" ... No such file or directory`, which
    /// reads like a lost file rather than an unsupported spelling.
    ///
    /// Three DDD bundles (2h3e, 2igy, 2obo) failed exactly that way in the
    /// 2026-09 prod wave. Their generator emitted the placeholder
    /// `trajectory_file_names = ["Pro_lig*.mdc"]` in precisely the bundles
    /// where it had no trajectories to enumerate, so the wildcard is also a
    /// reliable signal that the data is absent -- healthy bundles list all
    /// fifty `Pro_lig<N>.mdc` names. Saying so here turns a filesystem
    /// error into an actionable one, and says it before any work is done.
    fn check_no_wildcard(fieldname: &str, filename: &str) -> Option<String> {
        filename
            .chars()
            .find(|c| matches!(c, '*' | '?' | '['))
            .map(|c| {
                format!(
                    r#"{fieldname}: filename "{filename}" contains the wildcard '{c}'; globs are not expanded, so every file must be listed explicitly"#
                )
            })
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let Some(ext) = path.extension() else {
            bail!("No file extension")
        };
        let contents = read_file(path)?;
        if contents.is_empty() {
            bail!("File is empty")
        }
        match ext.to_str() {
            Some("json") => Self::from_json(&contents),
            Some("toml") => Self::from_toml(&contents),
            _ => bail!(r#"Unknown file extension "{}""#, ext.display()),
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
            trajectory_file_names: vec!["traj.xtc".to_string()],
            structure_file_name: "struct.pdb".to_string(),
            topology_file_name: "topology.tpr".to_string(),
            temperature_kelvin: 300,
            integration_timestep_fs: 2,
            sampling_frequency_ps: Some(10.),
            short_description: "<short_description> (required)".to_string(),
            description: Some("<longer description>".to_string()),
            software_name: "GROMACS".to_string(),
            software_version: "2024.4".to_string(),
            toml_version: Some(2),
            alias: Some("ABC123".to_string()),
            external_links: Some(vec![ExternalLink {
                url: "http://aol.com".to_string(),
                label: Some("My Link".to_string()),
            }]),
            run_commands: Some("gmx_mpi mdrun".to_string()),
            additional_files: Some(vec![AdditionalFile {
                file_name: "README.txt".to_string(),
                file_type: "User-defined file".to_string(),
                description: Some("Documentation of methods".to_string()),
            }]),
            forcefield: Some("CHARMM36m".to_string()),
            forcefield_comments: Some(
                "ligand params: CGenFF and SwissParam".to_string(),
            ),
            protonation_method: Some("PropKa".to_string()),
            pdb_id: Some("5emo".to_string()),
            uniprot_ids: Some(vec!["A0A0H2UWN8".to_string(), "S8G8I1".to_string()]),
            collections: Some(vec!["ATLAS".to_string()]),
            water: Some(Water {
                model: "TIP3P".to_string(),
                density_kg_m3: 986.,
            }),
            ligands: Some(vec![Ligand {
                name: "Foropafant".to_string(),
                smiles: Some(
                    "CC(C)C1=CC(=C(C(=C1)C(C)C)C2=CSC(=N2)N(CCN(C)C)CC3=CN=CC=C3)C(C)C"
                        .to_string(),
                ),
                inchi: None,
            }]),
            solutes: Some(vec![
                Solute {
                    name: "Na+".to_string(),
                    concentration_mol_liter: 0.15,
                },
                Solute {
                    name: "Cl-".to_string(),
                    concentration_mol_liter: 0.15,
                },
            ]),
            papers: None,
            dois: Some(vec!["10.1017/j.str.2019.08.032".to_string()]),
            is_embargoed: Some(true),
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
            trajectory_file_names: vec!["traj.xtc".to_string()],
            structure_file_name: "struct.pdb".to_string(),
            topology_file_name: "topology.tpr".to_string(),
            temperature_kelvin: 300,
            integration_timestep_fs: 2,
            sampling_frequency_ps: None,
            short_description: "<short_description> (required)".to_string(),
            description: None,
            software_name: "GROMACS".to_string(),
            software_version: "2024.4".to_string(),
            toml_version: None,
            alias: None,
            external_links: None,
            run_commands: None,
            additional_files: None,
            forcefield: None,
            forcefield_comments: None,
            protonation_method: None,
            pdb_id: None,
            uniprot_ids: None,
            collections: None,
            water: None,
            ligands: None,
            solutes: None,
            papers: None,
            dois: None,
            is_embargoed: None,
            contributors: None,
        }
    }
}

/// A pared-down public record: `Meta` plus the handful of database-only
/// fields (id, duration, replicate count, deprecation/embargo status) that
/// submitters never set but that a public catalog dump needs. Serialize-only
/// -- it exists for the nightly public JSON export, not for round-tripping
/// submissions, so unlike `Meta` it carries no `Deserialize` or `Validate`.
/// Deliberately excludes contributions, rmsd_values, rmsf_values,
/// uploaded_files, and processed_files, which the public API serializer
/// includes but which would needlessly bloat 50K+ files.
#[derive(Debug, Serialize)]
pub struct Summary {
    pub id: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_replicates: Option<i32>,

    pub is_deprecated: bool,

    pub is_placeholder: bool,

    pub creation_date: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseding_simulation_id: Option<i32>,

    #[serde(flatten)]
    pub meta: Meta,
}

impl Summary {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self).map_err(Into::into)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ExternalLink {
    #[validate(url)]
    pub url: String,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

    #[validate(custom(function = "validate_paper_year"))]
    pub year: u32,

    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,

    #[validate(regex(path = *constants::DOI_REGEX))]
    pub doi: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate)]
#[serde(deny_unknown_fields)]
#[validate(schema(
    function = "ligand_declares_identity",
    skip_on_field_errors = false
))]
pub struct Ligand {
    #[validate(regex(path = *constants::NOT_WHITESPACE_REGEX))]
    pub name: String,

    /// A ligand must carry a structure, but either notation will do. SMILES has
    /// no canonical form -- every toolkit canonicalizes differently -- while
    /// standard InChI has one reference implementation, so accepting both lets a
    /// submitter send what their tooling already produces and lets us store the
    /// canonical form regardless. At LEAST one is required -- declaring both is
    /// allowed and is checked for agreement on the ingest path, which is where
    /// OpenBabel is available. See `ligand_declares_identity`. Neither value is
    /// ever rewritten in the submitter's own file.
    #[validate(custom(function = "is_valid_smiles"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smiles: Option<String>,

    #[validate(custom(function = "is_valid_inchi"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inchi: Option<String>,
}

impl Ligand {
    /// The notation this ligand was declared with, for messages. Validation
    /// guarantees at least one is present, so the fallback is unreachable in
    /// practice and is a label rather than a panic.
    pub fn identity(&self) -> &str {
        self.smiles
            .as_deref()
            .or(self.inchi.as_deref())
            .unwrap_or("<no structure declared>")
    }

    /// Which notations the submitter actually supplied, recorded before
    /// anything is derived from one to fill the other -- afterwards both are
    /// present and the distinction is gone. Stored as `md_ligand`
    /// `declared_identity`.
    pub fn declared_identity(&self) -> Option<&'static str> {
        match (self.smiles.is_some(), self.inchi.is_some()) {
            (true, true) => Some("both"),
            (true, false) => Some("smiles"),
            (false, true) => Some("inchi"),
            (false, false) => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Solute {
    #[validate(custom(function = "validate_solute_name"))]
    pub name: String,

    #[validate(
       range(
           exclusive_min = constants::SOLUTE_CONCENTRATION_EXCLUSIVE_MIN,
           exclusive_max = constants::SOLUTE_CONCENTRATION_EXCLUSIVE_MAX
       )
    )]
    pub concentration_mol_liter: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Water {
    #[validate(custom(function = "validate_water_model"))]
    pub model: String,

    #[validate(
       range(min = constants::WATER_DENSITY_MIN, max = constants::WATER_DENSITY_MAX)
    )]
    pub density_kg_m3: f64,
}

fn validate_paper_year(year: u32) -> Result<(), ValidationError> {
    let max_year = (chrono::Utc::now().year() + 5) as u32;
    if year < 1900 || year > max_year {
        return Err(ValidationError::new("paper_year"));
    }
    Ok(())
}

fn validate_dois(dois: &[String]) -> Result<(), ValidationError> {
    if !dois.iter().all(|val| constants::DOI_REGEX.is_match(val)) {
        return Err(ValidationError::new("doi"));
    }
    Ok(())
}

fn validate_water_model(model: &str) -> Result<(), ValidationError> {
    if !constants::VALID_WATER_MODEL.contains(&model) {
        return Err(ValidationError::new("water_model").with_message(
            format!(
                r#"invalid, choose from {}"#,
                constants::VALID_WATER_MODEL.join(", ")
            )
            .into(),
        ));
    }
    Ok(())
}

fn validate_solute_name(name: &str) -> Result<(), ValidationError> {
    if !constants::VALID_SOLUTE_NAME.contains(&name) {
        return Err(ValidationError::new("solute_name").with_message(
            format!(
                r#"invalid, choose from {}"#,
                constants::VALID_SOLUTE_NAME.join(", ")
            )
            .into(),
        ));
    }
    Ok(())
}

fn validate_uniprot_ids(ids: &[String]) -> Result<(), ValidationError> {
    if !ids
        .iter()
        .all(|val| constants::NOT_WHITESPACE_REGEX.is_match(val))
    {
        return Err(ValidationError::new("uniprot_id"));
    }
    Ok(())
}

fn validate_collections(names: &[String]) -> Result<(), ValidationError> {
    if !names
        .iter()
        .all(|val| constants::NOT_WHITESPACE_REGEX.is_match(val))
    {
        return Err(ValidationError::new("collection"));
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
                    let fld = join_field(&format!("{field}[{}]", num + 1), sub_fld);
                    messages.extend(handle_validation_error_kind(&fld, err_kind));
                }
            }
        }
        // Struct(Box<ValidationErrors>)
        ValidationErrorsKind::Struct(validation_errors) => {
            for (sub_fld, err_kind) in validation_errors.errors() {
                let fld = join_field(field, sub_fld);
                messages.extend(handle_validation_error_kind(&fld, err_kind));
            }
        }
    };
    messages
}

// --------------------------------------------------
/// `validator` files a struct-level (`schema`) error under the synthetic field
/// name `__all__`, which is about the struct rather than any one of its fields.
/// Appending it would read as a field called `__all__`, so the parent path is
/// used unchanged.
fn join_field(parent: &str, sub: &str) -> String {
    if sub == "__all__" {
        parent.to_string()
    } else {
        format!("{parent}.{sub}")
    }
}

// --------------------------------------------------
fn format_validation_error(err: &ValidationError) -> String {
    let given = match err.params.get("value") {
        Some(val) => serde_json::to_string(val).unwrap_or_default(),
        _ => String::new(),
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
                _ => String::new(),
            }
        }
        Borrowed("range") => {
            let min = err.params.get("min");
            let max = err.params.get("max");
            let exclusive_min = err.params.get("exclusive_min");
            let exclusive_max = err.params.get("exclusive_max");
            match (min, max, exclusive_min, exclusive_max) {
                (Some(x), None, None, None) => format!("must be >= {x}"),
                (None, Some(x), None, None) => format!("must be <= {x}"),
                (Some(x), Some(y), None, None) => {
                    if x == y {
                        format!("must be = {x}")
                    } else {
                        format!("must be >= {x} and <= {y}")
                    }
                }
                (None, None, Some(x), None) => format!("must be > {x}"),
                (None, None, None, Some(x)) => format!("must be < {x}"),
                (None, None, Some(x), Some(y)) => format!("must be > {x} and < {y}"),
                _ => String::new(),
            }
        }
        _ => err.message.as_deref().unwrap_or("invalid").to_string(),
    };

    if given.is_empty() {
        // No `value` param: a struct-level rule, which is about the whole
        // table rather than one field, so there is no value to quote.
        message
    } else {
        format!("value {given} {message}")
    }
}

// --------------------------------------------------
/// A ligand must be identifiable. Only the *presence* of a notation is checked
/// here: whether a declared SMILES and InChI agree with each other is a
/// different question, it needs OpenBabel, and it therefore lives on the ingest
/// path (`mdr-process`'s `load_canonical_meta`) rather than in this pure check.
/// So `mdr-meta check` reports a missing or malformed value and says nothing
/// about agreement -- that split is deliberate, not an oversight.
pub fn ligand_declares_identity(
    ligand: &Ligand,
) -> std::result::Result<(), ValidationError> {
    if ligand.smiles.is_none() && ligand.inchi.is_none() {
        return Err(ValidationError::new("no_ligand_structure")
            .with_message(Borrowed("must declare either smiles or inchi")));
    }
    Ok(())
}

// --------------------------------------------------
/// A standard InChI, checked by its prefix and layer shape rather than by
/// round-tripping it. `InChI=1S/` is the standard form; a non-standard InChI
/// (`InChI=1/`) is refused because two of them are not comparable, which is the
/// whole reason for preferring InChI over SMILES here.
pub fn is_valid_inchi(inchi: &str) -> std::result::Result<(), ValidationError> {
    let invalid = |msg: &'static str| {
        ValidationError::new("invalid_inchi").with_message(Borrowed(msg))
    };

    let Some(rest) = inchi.strip_prefix("InChI=1S/") else {
        return Err(if inchi.starts_with("InChI=1/") {
            invalid("must be a STANDARD InChI (InChI=1S/), not InChI=1/")
        } else {
            invalid(r#"must begin with "InChI=1S/""#)
        });
    };

    // The formula layer is the only one guaranteed present, so an InChI with
    // nothing after the prefix carries no structure at all.
    if rest.split('/').next().unwrap_or_default().is_empty() {
        return Err(invalid("has no formula layer"));
    }
    if inchi.chars().any(char::is_whitespace) {
        return Err(invalid("must not contain whitespace"));
    }
    Ok(())
}

// --------------------------------------------------
/// Reading into a `graph::Builder` rather than a `write::Writer` follower buys
/// the ring-closure check for free: `read` alone only rejects bad syntax, so a
/// dangling or unmatched rnum ("c1ccccc", "C1CC") parses happily, while
/// `build` reconciles the rnums and reports it. Neither checks valence -- a
/// seven-bond carbon still passes here, as it does in OpenBabel.
pub fn is_valid_smiles(smiles: &str) -> std::result::Result<(), ValidationError> {
    let invalid = || ValidationError::new("invalid_smiles");
    let mut builder = purr::graph::Builder::new();
    purr::read::read(smiles, &mut builder, None).map_err(|_| invalid())?;
    builder.build().map(|_| ()).map_err(|_| invalid())
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

        #[test]
        fn invalid_doi_fails_regex(s in "[a-zA-Z]{5,15}") {
            // No "10.<digits>/" prefix at all, so this can never match --
            // guaranteed invalid by construction, not by exclusion.
            prop_assert!(!constants::DOI_REGEX.is_match(&s));
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
        fn valid_timestep_no_error(timestep in 1u32..=20u32) {
            let mut meta = base_meta();
            meta.integration_timestep_fs = timestep;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("integration_timestep_fs:"));
            prop_assert!(!has_error, "Unexpected timestep error for {timestep}: {errors:?}");
        }

        #[test]
        fn out_of_range_timestep_produces_error(timestep in prop_oneof![
            0u32..1u32,
            21u32..=u32::MAX,
        ]) {
            let mut meta = base_meta();
            meta.integration_timestep_fs = timestep;
            let errors = meta.check(None);
            let has_error = errors.iter().any(|e| e.starts_with("integration_timestep_fs:"));
            prop_assert!(has_error, "Expected timestep error for timestep={timestep}");
        }

        // --- Range: sampling_frequency_ps ---
        //
        // The floor moved from 0.001 ps to 1 ps on 2026-09-08, when 1 ps
        // became the absolute minimum spacing MDRepo will record. Worth a test
        // because nothing covered this range before and the bound is now
        // load-bearing: it is what stops a sub-picosecond declaration being
        // used to authorise a sub-picosecond measurement.

        #[test]
        fn valid_sampling_frequency_no_error(sampling in 1.0f64..=100_000.0f64) {
            let mut meta = base_meta();
            meta.sampling_frequency_ps = Some(sampling);
            let errors = meta.check(None);
            let has_error = errors
                .iter()
                .any(|e| e.starts_with("sampling_frequency_ps:"));
            prop_assert!(!has_error, "Unexpected sampling error for {sampling}: {errors:?}");
        }

        #[test]
        fn out_of_range_sampling_frequency_produces_error(sampling in prop_oneof![
            0.0f64..1.0f64,
            100_000.001f64..1e9f64,
        ]) {
            let mut meta = base_meta();
            meta.sampling_frequency_ps = Some(sampling);
            let errors = meta.check(None);
            let has_error = errors
                .iter()
                .any(|e| e.starts_with("sampling_frequency_ps:"));
            prop_assert!(has_error, "Expected sampling error for {sampling}");
        }

        // Absent is not an error: the field is optional and only 73 of the
        // 97,809 released simulations declare it at all.
        #[test]
        fn absent_sampling_frequency_no_error(timestep in 1u32..=20u32) {
            let mut meta = base_meta();
            meta.integration_timestep_fs = timestep;
            meta.sampling_frequency_ps = None;
            let errors = meta.check(None);
            let has_error = errors
                .iter()
                .any(|e| e.starts_with("sampling_frequency_ps:"));
            prop_assert!(!has_error, "Unexpected sampling error when unset: {errors:?}");
        }

        // --- Choice: Water model ---

        #[test]
        fn valid_water_model_passes(
            model in proptest::sample::select(constants::VALID_WATER_MODEL),
        ) {
            let water = Water {
                model: model.to_string(),
                density_kg_m3: 1000.0,
            };
            prop_assert!(water.validate().is_ok(), "Model '{model}' should be valid");
        }

        #[test]
        fn invalid_water_model_fails(s in "[a-zA-Z]{10,20}") {
            // Guaranteed not in VALID_WATER_MODEL by construction: every
            // multi-word/versioned entry contains a space, slash, hyphen, or
            // digit, and the longest pure-letter entry ("iAMOEBA") is 7 chars.
            let water = Water {
                model: s.clone(),
                density_kg_m3: 1000.0,
            };
            prop_assert!(water.validate().is_err(), "Model '{s}' should be invalid");
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

        // --- Choice: Solute name ---

        #[test]
        fn valid_solute_name_passes(
            name in proptest::sample::select(constants::VALID_SOLUTE_NAME),
        ) {
            let solute = Solute {
                name: name.to_string(),
                concentration_mol_liter: 0.15,
            };
            prop_assert!(solute.validate().is_ok(), "Name '{name}' should be valid");
        }

        #[test]
        fn invalid_solute_name_fails(s in "[a-zA-Z]{6,20}") {
            // Guaranteed not in VALID_SOLUTE_NAME by construction: the only
            // multi-char entry is "Phosphoric acid" (has a space), and the
            // longest pure-letter entry ("Urea") is 4 chars.
            let solute = Solute {
                name: s.clone(),
                concentration_mol_liter: 0.15,
            };
            prop_assert!(solute.validate().is_err(), "Name '{s}' should be invalid");
        }

        // --- Range: Solute concentration ---

        #[test]
        fn valid_solute_concentration_passes(conc in 0.001f64..0.999f64) {
            let solute = Solute {
                name: "Na".to_string(),
                concentration_mol_liter: conc,
            };
            prop_assert!(solute.validate().is_ok());
        }

        #[test]
        fn out_of_range_solute_concentration_fails(conc in prop_oneof![
            -10.0f64..=0.0f64,
            1.0f64..10.0f64,
        ]) {
            let solute = Solute {
                name: "Na+".to_string(),
                concentration_mol_liter: conc,
            };
            prop_assert!(solute.validate().is_err());
        }

        // --- Range: Paper year ---

        #[test]
        fn valid_paper_year_passes(year in 1900u32..=(chrono::Utc::now().year() as u32 + 5)) {
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
            (chrono::Utc::now().year() as u32 + 6)..=u32::MAX,
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
            timestep in 1u32..=20u32,
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
            timestep in 1u32..=20u32,
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
            meta.trajectory_file_names = vec![filename.clone()];
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
    const TOML_MINIMAL: &str = "tests/inputs/metadata/minimal.toml";

    /// The minimal valid document plus one appended table, read from the
    /// fixture rather than inlined so it cannot drift from it.
    fn minimal_plus(table: &str) -> String {
        let base = std::fs::read_to_string(TOML_MINIMAL).expect("minimal.toml");
        format!("{base}\n{table}\n")
    }

    use super::{
        AdditionalFile, DateTime, Ligand, Meta, MetaCheckOptions, Summary, Utc,
        is_valid_inchi, is_valid_smiles,
    };
    use crate::constants;
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
        assert_eq!(meta.trajectory_file_names, vec!["5aom.xtc"]);
        assert_eq!(meta.structure_file_name, "5aom_cleaned.pdb");
        assert_eq!(meta.topology_file_name, "5aom_gromacs_cleaned.top");
        assert_eq!(meta.temperature_kelvin, 300);
        assert_eq!(meta.integration_timestep_fs, 2);
        assert_eq!(meta.software_name, "AMBER");
        assert_eq!(
            meta.uniprot_ids,
            Some(vec!["P04637".to_string(), "Q9Y5S2".to_string()])
        );

        let water = meta.water.as_ref().expect("water should be Some");
        assert_eq!(water.model, "TIP3P");
        assert_eq!(water.density_kg_m3, 1000.0);

        let links = meta
            .external_links
            .as_ref()
            .expect("external_links should be Some");
        assert_eq!(links.len(), 1);
        let link = links.first().expect("link should exist");
        assert_eq!(link.url, "https://zenodo.org/records/7711953");
        assert_eq!(link.label, Some("Zenodo".to_string()));

        let ligands = meta.ligands.as_ref().expect("ligands should be Some");
        assert_eq!(ligands.len(), 2);
        let ligand = ligands.first().expect("ligand should exist");
        assert_eq!(ligand.name, "FY8".to_string());
        assert_eq!(
            ligand.smiles.as_deref(),
            Some("Oc1ccc(Cl)cc1NC(=O)C2CCNCC2")
        );
        assert_eq!(ligand.inchi, None);

        let solutes = meta.solutes.as_ref().expect("solutes should be Some");
        assert_eq!(solutes.len(), 2);
        let solute = solutes.first().expect("solute should exist");
        assert_eq!(solute.name, "Na+".to_string());
        assert_eq!(solute.concentration_mol_liter, 0.15);

        let contributors = meta
            .contributors
            .as_ref()
            .expect("contributors should be Some");
        assert_eq!(contributors.len(), 1);
        let c = contributors.first().expect("contributor should exist");
        assert_eq!(c.name, "Alex Leifson".to_string());
        assert_eq!(c.orcid, Some("0000-0003-2819-749X".to_string()));
        assert_eq!(c.email, Some("alex@aol.com".to_string()));
        assert_eq!(c.institution, Some("University of Montreal".to_string()));

        let uniprot_ids = meta
            .uniprot_ids
            .as_ref()
            .expect("uniprot_ids should be Some");
        assert_eq!(uniprot_ids.len(), 2);
        assert_eq!(
            uniprot_ids,
            &vec!["P04637".to_string(), "Q9Y5S2".to_string()]
        );

        let dois = meta.dois.as_ref().expect("dois should be Some");
        assert_eq!(dois.len(), 1);
        assert_eq!(
            dois.first().expect("doi should exist"),
            "10.1038/s43588-024-00627-2"
        );

        let papers = meta.papers.as_ref().expect("papers should be Some");
        assert_eq!(papers.len(), 1);
        assert_eq!(
            papers.first().expect("paper should exist").title,
            "MISATO: machine learning dataset of protein–ligand complexes \
            for structure-based drug discovery"
                .to_string()
        );

        let files = meta
            .additional_files
            .as_ref()
            .expect("additional_files should be Some");
        assert_eq!(files.len(), 1);
        let file = files.first().expect("file should exist");
        assert_eq!(file.file_type, "Structure".to_string());
        assert_eq!(file.file_name, "5aom_gromacs_cleaned.gro".to_string());
        assert_eq!(
            file.description,
            Some(".gro format of the structure file".to_string())
        );

        let errors = meta.check(None);
        assert!(&errors.is_empty());

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
            r#"collections: value ["   "] invalid"#,
            r#"contributors[1].email: value "alex" invalid"#,
            r#"contributors[1].orcid: value "0000-2819-749X" invalid"#,
            r#"dois: value ["1038/s43588-024-00627-2"] invalid"#,
            r#"external_links[1].label: value " " invalid"#,
            r#"external_links[1].url: value "zenodo.org/records/7711953" invalid"#,
            r#"forcefield: value " " invalid"#,
            r#"integration_timestep_fs: value 2000 must be >= 1 and <= 20"#,
            r#"lead_contributor_orcid: value "0000-0001-9961-144" invalid"#,
            r#"ligands[1].smiles: value "smiles_string" invalid"#,
            r#"pdb_id: value "5am" invalid"#,
            r#"short_description: value " " invalid"#,
            r#"solutes[1].concentration_mol_liter: value 0.0 must be > 0.0 and < 1.0"#,
            r#"solutes[1].name: value " " invalid, choose from Cl-, Cl, K, K+, Na, Na+, Phosphoric acid, Urea"#,
            r#"solutes[2].concentration_mol_liter: value 1.15 must be > 0.0 and < 1.0"#,
            r#"solutes[2].name: value "Chloride" invalid, choose from Cl-, Cl, K, K+, Na, Na+, Phosphoric acid, Urea"#,
            r#"structure_file_name: filename " " is missing extension"#,
            r#"temperature_kelvin: value 0 must be >= 275 and <= 700"#,
            r#"toml_version: value 4 must be = 2"#,
            r#"topology_file_name: filename " " is missing extension"#,
            r#"trajectory_file_names[1]: filename " " is missing extension"#,
            r#"uniprot_ids: value ["P04637","Q9Y5S2","   "] invalid"#,
            r#"water.density_kg_m3: value 1000000.0 must be >= 900.0 and <= 1100.0"#,
            concat!(
                r#"water.model: value "XYZ" invalid, choose from AMOEBA, BF, BK3, BMW, "#,
                "COS/G2, COS/G3, CVFF, DC, ELBA, EVB, F3C, HIPPO, KKY, LEWIS, MARTINI polarizable water, ",
                "MARTINI water, MB-pol, MCY, MS-EVB, OPC, OPC3, OSS2, POL3, RWK, ReaxFF, SCME, SDK/CMM, ",
                "SPC, SPC/E, SPC/Fd, SPC/Fw, ST2, SWM4-NDP, SWM6, TIP3P, TIP3P-FB, TIP3P/Fs, TIP4P, TIP4P-CG, ",
                "TIP4P-D, TIP4P-FB, TIP4P/2005, TIP4P/Ew, TIP4P/Ice, TIP5P, TIP5P/2018, TIP5P/E, TIP6P, ",
                "TTM2-F, TTM3-F, TTM4-F, iAMOEBA, mW, q-SPC/Fw, q-TIP4P/F"
            ),
            r#"Filename " " is duplicated 4 times"#,
            r#"software_name: "AMBERX" invalid, choose from ACEMD, AMBER, CHARMM, CUSTOM, GROMACS, NAMD, SPONGE"#,
        ];
        assert_eq!(errors.len(), expected.len());
        for message in expected {
            dbg!(&message);
            assert!(errors.contains(&message.to_string()));
        }
        Ok(())
    }

    // --- from_file error cases ---

    #[test]
    fn meta_from_file_no_extension() {
        let res = Meta::from_file(&PathBuf::from("no_such_file_without_ext"));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("No file extension"));
    }

    #[test]
    fn meta_from_file_empty() {
        let res = Meta::from_file(&PathBuf::from("tests/inputs/metadata/empty.txt"));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("File is empty"));
    }

    // --- all_filenames ---

    #[test]
    fn meta_all_filenames_without_additional() {
        let meta = Meta::example_minimal();
        let filenames = meta.all_filenames();
        assert!(filenames.contains(&"struct.pdb".to_string()));
        assert!(filenames.contains(&"topology.tpr".to_string()));
        assert!(filenames.contains(&"traj.xtc".to_string()));
        assert_eq!(filenames.len(), 3);
    }

    #[test]
    fn meta_all_filenames_with_additional() {
        let meta = Meta::example();
        let filenames = meta.all_filenames();
        assert!(filenames.contains(&"struct.pdb".to_string()));
        assert!(filenames.contains(&"topology.tpr".to_string()));
        assert!(filenames.contains(&"traj.xtc".to_string()));
        assert!(filenames.contains(&"README.txt".to_string()));
        assert_eq!(filenames.len(), 4);
    }

    // --- wildcards in referenced filenames ---

    #[test]
    fn check_no_wildcard_accepts_a_real_filename() {
        assert!(
            Meta::check_no_wildcard("trajectory_file_names[1]", "Pro_lig7.mdc")
                .is_none()
        );
    }

    #[test]
    fn check_no_wildcard_rejects_star_question_and_bracket() {
        for name in ["Pro_lig*.mdc", "Pro_lig?.mdc", "Pro_lig[0-9].mdc"] {
            let msg = Meta::check_no_wildcard("trajectory_file_names[1]", name)
                .unwrap_or_else(|| panic!("{name} should be rejected"));
            assert!(msg.contains("globs are not expanded"), "{msg}");
            assert!(msg.contains(name), "{msg}");
        }
    }

    /// Reproduces 2h3e/2igy/2obo from the 2026-09 DDD prod wave, whose
    /// generator wrote the pattern instead of enumerating the trajectories.
    /// Before this check the wildcard passed metadata validation and only
    /// failed later as a filesystem ENOENT on a name that never existed.
    #[test]
    fn wildcard_trajectory_is_reported_by_check() {
        let mut meta = Meta::example_minimal();
        meta.trajectory_file_names = vec!["Pro_lig*.mdc".to_string()];

        let errors = meta.check(None);
        assert!(
            errors.iter().any(|e| e.contains("globs are not expanded")),
            "expected a wildcard error, got {errors:?}"
        );
    }

    #[test]
    fn enumerated_trajectories_produce_no_wildcard_error() {
        let mut meta = Meta::example_minimal();
        meta.trajectory_file_names =
            vec!["Pro_lig1.xtc".to_string(), "Pro_lig2.xtc".to_string()];

        let errors = meta.check(None);
        assert!(
            !errors.iter().any(|e| e.contains("globs are not expanded")),
            "unexpected wildcard error in {errors:?}"
        );
    }

    // --- MetaCheckOptions ---

    #[test]
    fn meta_check_allow_no_pdb_uniprot_suppresses_error() {
        let meta = Meta::example_minimal();
        let errors = meta.check(Some(MetaCheckOptions {
            allow_no_pdb_uniprot: true,
        }));
        assert!(
            !errors
                .iter()
                .any(|e| e.contains("Missing PDB and Uniprot IDs")),
            "Expected no ID error with allow_no_pdb_uniprot=true, got: {errors:?}"
        );
    }

    #[test]
    fn meta_check_missing_pdb_uniprot_error() {
        let meta = Meta::example_minimal();
        let errors = meta.check(None);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Missing PDB and Uniprot IDs")),
            "Expected missing ID error, got: {errors:?}"
        );
    }

    // --- is_valid_inchi ---

    #[test]
    fn test_is_valid_inchi_standard() {
        // 10gs's declared ligand, from collection 5.
        assert!(
            is_valid_inchi(
                "InChI=1S/C23H27N3O6S/c24-17(22(29)30)11-12-19(27)25-18(14-33-13-15-7-3-1-4-8-15)21(28)26-20(23(31)32)16-9-5-2-6-10-16"
            )
            .is_ok()
        );
        assert!(is_valid_inchi("InChI=1S/CH4/h1H4").is_ok());
    }

    #[test]
    fn test_is_valid_inchi_rejects_nonstandard() {
        // A non-standard InChI is refused rather than tolerated: two of them are
        // not comparable, which is the entire reason for preferring InChI here.
        let err = is_valid_inchi("InChI=1/CH4/h1H4").unwrap_err();
        assert!(
            err.message
                .as_deref()
                .unwrap_or_default()
                .contains("STANDARD"),
            "message was {:?}",
            err.message
        );
    }

    #[test]
    fn test_is_valid_inchi_rejects_junk() {
        // A bare SMILES in the inchi field is the mistake worth catching.
        assert!(is_valid_inchi("CC(C)C1=CC(=C1)C(C)C").is_err());
        assert!(is_valid_inchi("").is_err());
        assert!(is_valid_inchi("InChI=1S/").is_err(), "no formula layer");
        assert!(is_valid_inchi("InChI=1S/CH4 /h1H4").is_err(), "whitespace");
    }

    // --- ligand_declares_identity ---

    #[test]
    fn test_ligand_accepts_either_notation_alone() {
        for (smiles, inchi) in [
            (Some("CC".to_string()), None),
            (None, Some("InChI=1S/C2H6/c1-2/h1-2H3".to_string())),
            (
                Some("CC".to_string()),
                Some("InChI=1S/C2H6/c1-2/h1-2H3".to_string()),
            ),
        ] {
            let ligand = Ligand {
                name: "ethane".to_string(),
                smiles,
                inchi,
            };
            assert!(ligand.validate().is_ok(), "rejected {ligand:?}");
        }
    }

    #[test]
    fn test_ligand_with_neither_notation_is_rejected() {
        let ligand = Ligand {
            name: "mystery".to_string(),
            smiles: None,
            inchi: None,
        };
        let err = ligand.validate().unwrap_err();
        assert!(err.errors().contains_key("__all__"));
    }

    #[test]
    fn test_ligand_missing_structure_names_itself_in_meta_check() {
        // The whole point of the struct-level rule is the message a submitter
        // sees, and `__all__` must not leak into it as though it were a field.
        let mut meta = Meta::example();
        meta.ligands = Some(vec![Ligand {
            name: "mystery".to_string(),
            smiles: None,
            inchi: None,
        }]);
        let messages = meta.check(None);
        assert!(
            messages
                .iter()
                .any(|m| m == "ligands[1]: must declare either smiles or inchi"),
            "got {messages:?}"
        );
    }

    #[test]
    fn test_ligand_declared_identity_labels() {
        let with = |smiles: Option<&str>, inchi: Option<&str>| Ligand {
            name: "x".to_string(),
            smiles: smiles.map(String::from),
            inchi: inchi.map(String::from),
        };
        assert_eq!(with(Some("CC"), None).declared_identity(), Some("smiles"));
        assert_eq!(
            with(None, Some("InChI=1S/C2H6/c1-2/h1-2H3")).declared_identity(),
            Some("inchi")
        );
        assert_eq!(
            with(Some("CC"), Some("InChI=1S/C2H6/c1-2/h1-2H3")).declared_identity(),
            Some("both")
        );
        assert_eq!(with(None, None).declared_identity(), None);
    }

    // --- deny_unknown_fields on the nested tables ---

    #[test]
    fn test_ligand_accepts_an_inchi_key() {
        // Before this change the key parsed and was silently discarded.
        let meta = Meta::from_toml(&minimal_plus(
            "[[ligands]]\nname = \"ethane\"\ninchi = \"InChI=1S/C2H6/c1-2/h1-2H3\"",
        ))
        .expect("inchi-only ligand should parse");
        let messages = meta.check(Some(MetaCheckOptions {
            allow_no_pdb_uniprot: true,
        }));
        let ligands = meta.ligands.expect("ligands");
        assert_eq!(
            ligands[0].inchi.as_deref(),
            Some("InChI=1S/C2H6/c1-2/h1-2H3")
        );
        assert_eq!(ligands[0].smiles, None);
        assert!(
            !messages.iter().any(|m| m.contains("ligand")),
            "got {messages:?}"
        );
    }

    #[test]
    fn test_stray_ligand_key_is_now_rejected() {
        // `inchii` used to validate clean while the value disappeared. This is
        // the typo class `deny_unknown_fields` exists to catch.
        let res = Meta::from_toml(&minimal_plus(
            "[[ligands]]\nname = \"ethane\"\nsmiles = \"CC\"\ninchii = \"InChI=1S/C2H6\"",
        ));
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("inchii"),
            "message did not name the key: {err}"
        );
    }

    #[test]
    fn test_stray_key_rejected_in_every_nested_table() {
        for table in [
            "[[contributors]]\nname = \"A\"\nnope = 1",
            "[[solutes]]\nname = \"NaCl\"\nconcentration_mol_liter = 0.1\nnope = 1",
            "[water]\nmodel = \"TIP3P\"\ndensity_kg_m3 = 1000.0\nnope = 1",
            "[[external_links]]\nurl = \"https://example.com\"\nnope = 1",
            "[[additional_files]]\nfile_name = \"a\"\nfile_type = \"b\"\nnope = 1",
            // `papers` was the one nested table this loop missed.
            "[[papers]]\ntitle = \"A paper\"\nauthors = \"A. Author\"\nnope = 1",
        ] {
            let res = Meta::from_toml(&minimal_plus(table));
            assert!(res.is_err(), "accepted a stray key in: {table}");
            assert!(
                res.unwrap_err().to_string().contains("nope"),
                "did not name the key in: {table}"
            );
        }
    }

    // --- a ligand must declare a structure: the corpus shape ---

    /// The shape the DDD contributor actually ships, twice over now: a
    /// `[[ligands]]` table carrying a name and nothing else. All 15,525 files
    /// in the 2026-09-08 delivery are like this -- zero `smiles`, zero `inchi`,
    /// zero `formula`.
    ///
    /// The document is well-formed, so it parses; the rule that catches it is
    /// the struct-level one, and it has to survive both. Before `cba350b` this
    /// failed at DESERIALIZATION with "missing field smiles", so `Meta::check`
    /// never ran and the submitter got a serde error instead of a sentence.
    #[test]
    fn test_a_ligand_with_only_a_name_is_reported_not_a_parse_error() {
        let meta = Meta::from_toml(&minimal_plus(
            "[[ligands]]\nname = \"2-HYDROXYETHYL DISULFIDE\"",
        ))
        .expect("a name-only ligand is well-formed TOML and must parse");

        assert_eq!(meta.ligands.as_ref().unwrap().len(), 1);
        assert_eq!(meta.ligands.as_ref().unwrap()[0].smiles, None);
        assert_eq!(meta.ligands.as_ref().unwrap()[0].inchi, None);

        assert!(
            meta.check(no_id_opts())
                .iter()
                .any(|m| m == "ligands[1]: must declare either smiles or inchi"),
            "{:?}",
            meta.check(no_id_opts())
        );
    }

    /// A blank name AND no structure, which is 585 of those 15,525 files.
    ///
    /// Both problems must be reported. `skip_on_field_errors = false` on the
    /// Ligand schema validator is what makes the structure complaint survive
    /// alongside the name one; flipping it to `true` would silently drop the
    /// second whenever the first fires, and nothing pinned that.
    #[test]
    fn test_a_blank_name_does_not_hide_the_missing_structure() {
        let meta =
            Meta::from_toml(&minimal_plus("[[ligands]]\nname = \"\"")).expect("parses");
        let messages = meta.check(no_id_opts());

        assert!(
            messages.iter().any(|m| m.starts_with("ligands[1].name")),
            "{messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|m| m == "ligands[1]: must declare either smiles or inchi"),
            "{messages:?}"
        );
    }

    /// `Ligand::identity()` falls back to the literal `<no structure declared>`
    /// and its only consumer is a WARNING in resolve_ligands, so a ligand with
    /// neither notation would print a placeholder rather than fail.
    ///
    /// Validation is what makes that unreachable, and validation is the only
    /// thing that makes it unreachable -- the type permits the state and a
    /// struct literal reaches it, as the line below shows. If the schema rule
    /// is ever relaxed, this is where the consequence is written down.
    #[test]
    fn test_the_identity_fallback_is_unreachable_through_validation_only() {
        let naked = Ligand {
            name: "mystery".into(),
            smiles: None,
            inchi: None,
        };

        assert_eq!(naked.identity(), "<no structure declared>");
        assert!(naked.validate().is_err(), "validation is the only guard");

        // Through the parse path the fallback cannot be reached, because the
        // document does not survive check().
        let meta = Meta::from_toml(&minimal_plus("[[ligands]]\nname = \"mystery\""))
            .expect("parses");
        assert!(!meta.check(no_id_opts()).is_empty());
    }

    // --- either notation alone: true here, and not true end to end ---

    /// `is_valid_inchi` accepts `InChI=1S/xyzzy`, and that is deliberate.
    ///
    /// It is a FORMAT check -- prefix, a non-empty first segment, no
    /// whitespace. The chemistry check needs a toolkit, so it lives on the
    /// ingest path where one is available, and `mdr-meta check` is pure Rust
    /// and reaches submitters without one. Asserted so that the next person to
    /// notice the hole reaches for the ingest-path check instead of building a
    /// half-parser here.
    #[test]
    fn test_is_valid_inchi_is_a_format_check_not_a_chemistry_check() {
        assert!(is_valid_inchi("InChI=1S/xyzzy").is_ok());
        assert!(is_valid_inchi("InChI=1S/C2H6/zzzz").is_ok());

        // What it does catch is a string that could not be an InChI at all.
        assert!(is_valid_inchi("InChI=1S/").is_err());
        assert!(is_valid_inchi("CCO").is_err());
    }

    // --- is_valid_smiles ---

    #[test]
    fn test_is_valid_smiles_valid() {
        assert!(is_valid_smiles("C").is_ok());
        assert!(is_valid_smiles("CC").is_ok());
        assert!(is_valid_smiles("Oc1ccc(Cl)cc1NC(=O)C2CCNCC2").is_ok());
    }

    #[test]
    fn test_is_valid_smiles_invalid() {
        assert!(is_valid_smiles("smiles_string").is_err());
        assert!(is_valid_smiles("not!valid").is_err());
        assert!(is_valid_smiles("").is_err());
        assert!(is_valid_smiles("   ").is_err());
        assert!(is_valid_smiles("CCO junk").is_err());
    }

    /// Unbalanced ring-closure digits parse as valid SMILES syntax and are only
    /// caught when the graph is built, so they get their own test.
    #[test]
    fn test_is_valid_smiles_rejects_unbalanced_rnum() {
        assert!(is_valid_smiles("c1ccccc").is_err());
        assert!(is_valid_smiles("C1CC").is_err());
        // Balanced rings of both digit forms still pass.
        assert!(is_valid_smiles("c1ccccc1").is_ok());
        assert!(is_valid_smiles("C%10CCCCC%10").is_ok());
    }

    // --- sampling_frequency_ps: the range check, at its edges ---

    /// The proptests either side of this file draw from `1.0..=100_000.0` and
    /// `0.0..1.0`, and proptest float strategies essentially never emit an
    /// endpoint, so `validator`'s inclusivity has never been asserted.
    ///
    /// The 0.995 case is deliberately the same number as
    /// `resolve_sampling_ps_does_not_recheck_its_own_return_value` over in
    /// mdr-process. That function will hand back a sub-1 ps value if it is ever
    /// given one; this is the check that stops it being given one. The two
    /// belong together.
    #[test]
    fn sampling_frequency_range_is_inclusive_at_both_ends() {
        let says_sampling = |value: Option<f64>| {
            let mut meta = Meta::example_minimal();
            meta.sampling_frequency_ps = value;
            meta.check(None)
                .iter()
                .any(|m| m.starts_with("sampling_frequency_ps:"))
        };

        let min = constants::SAMPLING_FREQUENCY_PS_MIN;
        let max = constants::SAMPLING_FREQUENCY_PS_MAX;

        assert!(!says_sampling(Some(min)), "exactly the minimum is valid");
        assert!(!says_sampling(Some(max)), "exactly the maximum is valid");

        for bad in [
            f64::from_bits(min.to_bits() - 1),
            f64::from_bits(max.to_bits() + 1),
            0.995,
            0.,
            -1.,
            f64::INFINITY,
        ] {
            assert!(says_sampling(Some(bad)), "{bad} must be reported");
        }
    }

    /// A hole, asserted as it stands. TOML 1.0 has a `nan` literal, and the
    /// range validator answers "in range" for it because both of its
    /// comparisons are false for NaN. So `sampling_frequency_ps = nan` parses
    /// and validates, and downstream it is worse than wrong: resolve_sampling_ps
    /// returns Ok(NaN), serde_json writes NaN as `null`, and the resulting
    /// duration.json then fails to read back into a non-Option f32 -- on every
    /// subsequent run, identically, from a cache the pipeline wrote itself.
    ///
    /// `inf` is caught, which is what makes this specific to NaN. The fix is an
    /// `is_finite()` guard next to the cross-field timestep check; when it
    /// lands, this test flips.
    #[test]
    fn a_nan_sampling_frequency_is_accepted_today() {
        let meta = Meta::from_toml(&minimal_plus("sampling_frequency_ps = nan"))
            .expect("TOML 1.0 has a nan literal");
        assert!(meta.sampling_frequency_ps.expect("parsed").is_nan());
        assert!(
            !meta
                .check(None)
                .iter()
                .any(|m| m.starts_with("sampling_frequency_ps:")),
            "NaN is not reported today -- this test flips when it is"
        );

        // The contrast that shows it is NaN specifically, not non-finite values
        // in general.
        let infinite = Meta::from_toml(&minimal_plus("sampling_frequency_ps = inf"))
            .expect("inf parses too");
        assert!(
            infinite
                .check(None)
                .iter()
                .any(|m| m.starts_with("sampling_frequency_ps:")),
            "inf IS caught by the range check"
        );
    }

    // --- example() / example_minimal() validity ---

    #[test]
    fn meta_example_is_valid() {
        let meta = Meta::example();
        let errors = meta.check(None);
        assert!(
            errors.is_empty(),
            "example() should produce no check errors, got: {errors:?}"
        );
    }

    #[test]
    fn meta_example_minimal_only_id_error() {
        let meta = Meta::example_minimal();
        let errors = meta.check(None);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Missing PDB and Uniprot IDs")),
            "Expected missing ID error, got: {errors:?}"
        );
        let other_errors: Vec<_> = errors
            .iter()
            .filter(|e| !e.contains("Missing PDB and Uniprot IDs"))
            .collect();
        assert!(
            other_errors.is_empty(),
            "Unexpected extra errors: {other_errors:?}"
        );
    }

    // --- software name/version validation ---

    #[test]
    fn meta_check_invalid_software_version() {
        let mut meta = Meta::example_minimal();
        meta.software_name = "AMBER".to_string();
        meta.software_version = "99.99".to_string();
        let errors = meta.check(None);
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with("software_version:") && e.contains("99.99")),
            "Expected invalid version error, got: {errors:?}"
        );
    }

    /// GROMACS names the first release of a series with no patch component, so
    /// a bare year is a real version for every series it ships. 2025 was
    /// entered as "2025.0" and the bare form was therefore refused -- which is
    /// the whole of why simulation 20615 sits among the invalid released
    /// TOMLs, and it is our defect, not the submitter's.
    ///
    /// The loop is over every series rather than 2025 alone: the same omission
    /// in 2027 would be just as silent, and this is the test that would catch
    /// it. 2016 through 2024 already pass and are included so the rule is
    /// stated once rather than asserted for the one year that broke.
    #[test]
    fn gromacs_accepts_a_bare_year_for_every_series_it_ships() {
        let bare_year_valid = |version: &str| {
            let mut meta = Meta::example_minimal();
            meta.pdb_id = Some("5aom".to_string());
            meta.software_name = "GROMACS".to_string();
            meta.software_version = version.to_string();
            !meta
                .check(None)
                .iter()
                .any(|e| e.starts_with("software_version:"))
        };

        for year in [
            "2016", "2018", "2019", "2020", "2021", "2022", "2023", "2024", "2025",
            "2026",
        ] {
            assert!(bare_year_valid(year), "GROMACS {year} must be accepted");
        }

        // Still a real check: a year GROMACS has never shipped is refused.
        assert!(!bare_year_valid("2017"));
        assert!(!bare_year_valid("2030"));
    }

    #[test]
    fn meta_check_valid_software_version() {
        let mut meta = Meta::example_minimal();
        meta.pdb_id = Some("5aom".to_string());
        meta.software_name = "AMBER".to_string();
        meta.software_version = "2020".to_string();
        // example_minimal()'s default topology_file_name is GROMACS-shaped
        // (.tpr); swap in an AMBER-native one so this test isn't also
        // exercising the format-combination check.
        meta.topology_file_name = "topology.prmtop".to_string();
        let errors = meta.check(None);
        assert!(
            !errors.iter().any(|e| e.starts_with("software_")),
            "Expected no software errors, got: {errors:?}"
        );
    }

    // --- file extension checks ---
    //
    // An extension that no engine recognizes at all can never be part of a
    // valid combination, so these are caught by the combination check (see
    // "structure/topology/trajectory combination checks" below) rather than
    // by an independent per-field allowlist -- there is no longer a
    // per-field "invalid extension" message distinct from that.

    #[test]
    fn meta_check_invalid_structure_extension() {
        let mut meta = Meta::example_minimal();
        meta.structure_file_name = "struct.xyz".to_string();
        let errors = meta.check(None);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("incompatible") && e.contains("xyz")),
            "Expected structure extension error, got: {errors:?}"
        );
    }

    #[test]
    fn meta_check_invalid_topology_extension() {
        let mut meta = Meta::example_minimal();
        meta.topology_file_name = "topology.xyz".to_string();
        let errors = meta.check(None);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("incompatible") && e.contains("xyz")),
            "Expected topology extension error, got: {errors:?}"
        );
    }

    #[test]
    fn meta_check_invalid_trajectory_extension() {
        let mut meta = Meta::example_minimal();
        meta.trajectory_file_names = vec!["traj.mp4".to_string()];
        let errors = meta.check(None);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("incompatible") && e.contains("mp4")),
            "Expected trajectory extension error, got: {errors:?}"
        );
    }

    #[test]
    fn meta_check_empty_trajectory_file_names_is_fatal() {
        // trajectory_file_names carries #[validate(length(min = 1))], so
        // this is already fatal via self.validate() before
        // format_combination() is ever reached -- its is_empty() check is
        // just declining to do meaningless combination work on data that's
        // already been flagged, not silently accepting an empty list.
        let mut meta = Meta::example_minimal();
        meta.trajectory_file_names = vec![];
        let errors = meta.check(None);
        assert!(
            errors.iter().any(
                |e| e.starts_with("trajectory_file_names:") && e.contains("length")
            ),
            "Expected a fatal length error for empty trajectory_file_names, got: {errors:?}"
        );
    }

    // --- structure/topology/trajectory combination checks ---

    fn no_id_opts() -> Option<MetaCheckOptions> {
        Some(MetaCheckOptions {
            allow_no_pdb_uniprot: true,
        })
    }

    #[test]
    fn meta_combination_matching_declared_engine_is_clean() {
        let mut meta = Meta::example_minimal();
        meta.software_name = "GROMACS".to_string();
        meta.software_version = "2024.4".to_string();
        meta.structure_file_name = "struct.gro".to_string();
        meta.topology_file_name = "topology.top".to_string();
        meta.trajectory_file_names = vec!["traj.xtc".to_string()];
        meta.additional_files = Some(vec![AdditionalFile {
            file_name: "run.tpr".to_string(),
            file_type: "Binary topology".to_string(),
            description: None,
        }]);
        let errors = meta.check(no_id_opts());
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn meta_combination_amber_declared_with_xtc_trajectory_is_clean() {
        // The real MISATO cohort in prod: software_name AMBER, structure and
        // topology in AMBER's own shape, but trajectory .xtc because
        // MISATO's published .h5 trajectories were converted to .xtc on
        // import. xtc is accepted for every engine (see
        // constants::ENGINE_FILE_FORMATS), so this is not a mismatch --
        // see item 23 in utils/TODO.md.
        let mut meta = Meta::example_minimal();
        meta.software_name = "AMBER".to_string();
        meta.software_version = "2020".to_string();
        meta.structure_file_name = "struct.pdb".to_string();
        meta.topology_file_name = "topology.top".to_string();
        meta.trajectory_file_names = vec!["traj.xtc".to_string()];
        let errors = meta.check(no_id_opts());
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn meta_combination_wrong_declared_engine_is_fatal() {
        // .gro/.top/.trr is GROMACS's convention; nothing about it matches
        // ACEMD's psf/pdb/xtc. Unlike a plain mismatch resolved by
        // COMMON_TRAJECTORY_EXTS (see the AMBER/xtc case above), .trr isn't
        // shared, so this is a genuine declared-vs-actual mismatch.
        let mut meta = Meta::example_minimal();
        meta.software_name = "ACEMD".to_string();
        meta.software_version = "3.7.3".to_string();
        meta.structure_file_name = "struct.gro".to_string();
        meta.topology_file_name = "topology.top".to_string();
        meta.trajectory_file_names = vec!["traj.trr".to_string()];
        let errors = meta.check(no_id_opts());
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with("software_name") && e.contains("GROMACS")),
            "Expected a mismatched-engine error naming GROMACS, got: {errors:?}"
        );
    }

    #[test]
    fn meta_combination_matching_no_engine_is_fatal() {
        let mut meta = Meta::example_minimal();
        meta.software_name = "GROMACS".to_string();
        meta.software_version = "2024.4".to_string();
        // .gro is GROMACS-only for structure; .psf is CHARMM/NAMD/ACEMD-only
        // for topology. No engine accepts both at once.
        meta.structure_file_name = "struct.gro".to_string();
        meta.topology_file_name = "topology.psf".to_string();
        meta.trajectory_file_names = vec!["traj.dcd".to_string()];
        let errors = meta.check(no_id_opts());
        assert!(
            errors.iter().any(|e| e.contains("incompatible")),
            "Expected an incompatible-combination error, got: {errors:?}"
        );
    }

    #[test]
    fn meta_combination_custom_software_is_exempt() {
        let mut meta = Meta::example_minimal();
        meta.software_name = "CUSTOM".to_string();
        meta.software_version = "NA".to_string();
        meta.structure_file_name = "struct.pdb".to_string();
        meta.topology_file_name = "topology.psf".to_string();
        meta.trajectory_file_names = vec!["traj.nc".to_string()];
        let errors = meta.check(no_id_opts());
        assert!(
            !errors.iter().any(|e| e.contains("incompatible")),
            "CUSTOM should be exempt from combination checking, got: {errors:?}"
        );
    }

    // --- multiple trajectory files ---

    #[test]
    fn meta_multiple_trajectory_files() -> Result<()> {
        let meta = Meta::from_file(&PathBuf::from(
            "tests/inputs/metadata/minimal_mult_traj.toml",
        ))?;
        assert_eq!(meta.trajectory_file_names.len(), 2);
        assert!(meta.trajectory_file_names.contains(&"5aom.xtc".to_string()));
        assert!(
            meta.trajectory_file_names
                .contains(&"5aom_2.xtc".to_string())
        );
        Ok(())
    }

    // --- duplicate filenames in additional_files ---

    #[test]
    fn meta_check_duplicate_in_additional_files() {
        let mut meta = Meta::example_minimal();
        let dup_name = "README.txt".to_string();
        meta.additional_files = Some(vec![
            AdditionalFile {
                file_name: dup_name.clone(),
                file_type: "Documentation".to_string(),
                description: None,
            },
            AdditionalFile {
                file_name: dup_name.clone(),
                file_type: "Documentation".to_string(),
                description: None,
            },
        ]);
        let errors = meta.check(None);
        assert!(
            errors.iter().any(|e| e.contains("is duplicated")),
            "Expected duplicate filename error, got: {errors:?}"
        );
    }

    // --- Summary ---

    #[test]
    fn summary_json_flattens_meta_and_adds_public_fields() -> Result<()> {
        let summary = Summary {
            id: 42,
            duration: Some(12.5),
            num_replicates: Some(3),
            is_deprecated: false,
            is_placeholder: false,
            creation_date: DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .expect("valid rfc3339 timestamp")
                .with_timezone(&Utc),
            superseding_simulation_id: None,
            meta: Meta::example_minimal(),
        };

        let json = summary.to_json()?;
        let value: serde_json::Value = serde_json::from_str(&json)?;

        assert_eq!(value["id"], 42);
        assert_eq!(value["duration"], 12.5);
        assert_eq!(value["num_replicates"], 3);
        assert_eq!(value["is_deprecated"], false);
        assert_eq!(value["creation_date"], "2024-01-01T00:00:00Z");

        // Meta's fields are flattened to the top level, not nested.
        assert_eq!(value["structure_file_name"], "struct.pdb");
        assert_eq!(value["software_name"], "GROMACS");

        // None-valued optional fields are omitted rather than serialized
        // as null.
        assert!(value.get("superseding_simulation_id").is_none());

        // The public export deliberately excludes these -- they aren't
        // fields on Summary, so this also guards against a future
        // `#[serde(flatten)]` of the full API-shaped struct reintroducing
        // them by accident.
        for excluded in [
            "contributions",
            "rmsd_values",
            "rmsf_values",
            "uploaded_files",
            "processed_files",
        ] {
            assert!(
                value.get(excluded).is_none(),
                "Summary should not serialize {excluded:?}"
            );
        }

        Ok(())
    }
}
