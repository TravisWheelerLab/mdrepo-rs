use anyhow::{Result, anyhow, bail};
use libmdrepo::metadata::{self, Meta, MetaCheckOptions};
use log::debug;
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

// --------------------------------------------------
/// Read the metadata and canonicalize its ligand SMILES in memory, so both the
/// validator (`purr`) and everything downstream see the canonical forms without
/// the on-disk TOML ever being rewritten. OpenBabel accepts non-standard
/// notation such as `[N+H3]` that `purr` rejects and normalizes it to `[NH3+]`,
/// so canonicalizing before `validate_meta` keeps the validator happy while the
/// submitter's file is preserved as sent. Shells out only when the metadata
/// actually carries ligands.
pub fn load_canonical_meta(
    meta_path: &Path,
    script_dir: &Path,
    uv: &Path,
) -> Result<Meta> {
    let mut meta = Meta::from_file(meta_path)?;

    // Only the ligands that declared a SMILES are sent, and results come back
    // BY POSITION, so carry each one's index rather than trusting the two lists
    // to line up -- a ligand declaring only an InChI is skipped here, and
    // without the index its neighbour would collect its canonical form.
    // Deriving the missing notation is resolution's job, not this function's.
    let declared: Vec<(usize, String)> = match meta.ligands.as_ref() {
        Some(ligands) if !ligands.is_empty() => ligands
            .iter()
            .enumerate()
            .filter_map(|(num, l)| l.smiles.clone().map(|smi| (num, smi)))
            .collect(),
        _ => return Ok(meta),
    };

    if declared.is_empty() {
        return Ok(meta);
    }

    let smiles: Vec<String> = declared.iter().map(|(_, smi)| smi.clone()).collect();
    let canonical = canonicalize_smiles(&smiles, script_dir, uv)?;

    let ligands = meta.ligands.as_mut().unwrap();
    for ((num, _), canon) in declared.iter().zip(canonical) {
        ligands[*num].smiles = Some(canon);
    }
    Ok(meta)
}

// --------------------------------------------------
/// What `resolve_ligand_identity.py` gives back for one ligand, in input order.
///
/// `smiles` is never optional here: filling `md_ligand`'s NOT NULL column from
/// an InChI-only declaration is the reason the script exists. The other three
/// are, because a molecule RDKit will not accept still has to be storable --
/// see the script's FAILURE POLICY.
#[derive(Debug, Clone, Deserialize)]
pub struct ResolvedIdentity {
    pub smiles: String,
    pub inchi: Option<String>,
    pub inchikey: Option<String>,
    pub identity_software: Option<String>,
}

// --------------------------------------------------
/// Fill in whichever ligand notation the submitter did not supply.
///
/// One `uv` spawn for the whole batch, like `canonicalize_smiles`: the RDKit
/// and OpenBabel imports cost about a second between them and neither is worth
/// paying per ligand.
///
/// Results come back BY POSITION, so the caller must not filter the input --
/// every ligand goes in, including ones that need nothing done to them, and
/// the two lists line up index for index. The script fails the whole batch
/// naming the ligand when a SMILES will not parse or an InChI-only ligand
/// cannot be read, which is the same shape `canonicalize_smiles.py` has always
/// had.
pub fn resolve_ligand_identity(
    ligands: &[metadata::Ligand],
    script_dir: &Path,
    uv: &Path,
) -> Result<Vec<ResolvedIdentity>> {
    if ligands.is_empty() {
        return Ok(vec![]);
    }

    let payload = serde_json::to_string(
        &ligands
            .iter()
            .map(|l| {
                serde_json::json!({
                    "name": l.name,
                    "smiles": l.smiles,
                    "inchi": l.inchi,
                })
            })
            .collect::<Vec<_>>(),
    )?;

    let script = script_dir.join("resolve_ligand_identity.py");
    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir)
        .arg("run")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    debug!("Running {cmd:?}");

    let mut child = cmd.spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("no stdin on {}", script.display()))?
        .write_all(payload.as_bytes())?;

    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "{} failed: {}",
            script.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let resolved: Vec<ResolvedIdentity> = serde_json::from_slice(&output.stdout)?;
    if resolved.len() != ligands.len() {
        bail!(
            "{} returned {} result(s) for {} ligand(s)",
            script.display(),
            resolved.len(),
            ligands.len()
        );
    }

    Ok(resolved)
}

// --------------------------------------------------
/// Batch-canonicalize SMILES with OpenBabel via `canonicalize_smiles.py`,
/// returning the canonical forms in input order. One `uv` spawn covers the whole
/// batch (the OpenBabel import alone costs ~1s). An unparsable SMILES makes the
/// script exit non-zero, surfaced here as a submitter-facing error.
fn canonicalize_smiles(
    smiles: &[String],
    script_dir: &Path,
    uv: &Path,
) -> Result<Vec<String>> {
    let script = script_dir.join("canonicalize_smiles.py");
    let mut cmd = Command::new(uv);
    cmd.current_dir(script_dir)
        .arg("run")
        .arg(&script)
        .args(smiles);
    debug!("Running {cmd:?}");

    let output = cmd.output()?;
    if !output.status.success() {
        bail!(
            "{} failed: {}",
            script.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let canonical: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.to_string())
        .collect();
    if canonical.len() != smiles.len() {
        bail!(
            "canonicalize_smiles.py returned {} line(s) for {} SMILES",
            canonical.len(),
            smiles.len()
        );
    }
    Ok(canonical)
}

// --------------------------------------------------
/// Check that a directory holds a processable simulation for the given (already
/// loaded) metadata: the metadata passes its own checks, and the files it
/// references are present and not all empty. This says nothing about how the
/// directory got here -- verifying an upload against its manifest is
/// `ticket::check_manifest`, and reading/canonicalizing the metadata is
/// `load_canonical_meta`.
pub fn validate_meta(
    dir: &Path,
    meta: &Meta,
    meta_check_opts: Option<MetaCheckOptions>,
) -> Result<Vec<String>> {
    if !dir.is_dir() {
        bail!(r#"Invalid directory "{}""#, dir.display())
    }

    debug!(r#"Processing input directory "{}""#, dir.display());

    let num_files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .count();
    debug!(
        "Found {num_files} file{}",
        if num_files == 1 { "" } else { "s" }
    );

    if num_files == 0 {
        bail!(r#""{}" is empty!"#, dir.display());
    }

    let mut errors = meta.check(meta_check_opts);

    // Cross-check the metadata's claims against the directory: every file it
    // references must exist. A missing file is reported like any other error
    // rather than aborting, so the caller sees the whole picture at once.
    let mut total_file_size = 0;
    for filename in &meta.all_filenames() {
        match dir.join(filename).metadata() {
            Ok(metadata) => total_file_size += metadata.len(),
            Err(e) => errors.push(format!(
                r#"Missing file "{filename}" referenced by metadata: {e}"#
            )),
        }
    }

    if total_file_size == 0 {
        errors.push("All local files are empty!".to_string());
    }

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    const MINIMAL_TOML: &str = r#"
        lead_contributor_orcid = "0000-0000-0000-0000"
        trajectory_file_names = ["sim.xtc"]
        structure_file_name = "sim.pdb"
        topology_file_name = "sim.top"
        temperature_kelvin = 300
        integration_timestep_fs = 2
        short_description = "A test simulation"
        software_name = "GROMACS"
        software_version = "2023"
        pdb_id = "1ABC"
        [water]
        model = "TIP3P"
        density_kg_m3 = 1000.0
    "#;

    fn minimal_meta() -> Meta {
        Meta::from_toml(MINIMAL_TOML).unwrap()
    }

    fn write_minimal_dir(dir: &Path) {
        fs::write(dir.join("mdrepo-metadata.toml"), MINIMAL_TOML).unwrap();
        fs::write(dir.join("sim.xtc"), b"trajectory").unwrap();
        fs::write(dir.join("sim.pdb"), b"structure").unwrap();
        fs::write(dir.join("sim.top"), b"topology").unwrap();
    }

    // A path that cannot exist, used to prove `load_canonical_meta` does not
    // spawn `uv` when there are no ligands to canonicalize -- a spawn attempt
    // would fail and turn these Ok cases into errors.
    fn bogus_uv() -> &'static Path {
        Path::new("/nonexistent/uv")
    }

    #[test]
    fn rejects_nonexistent_dir() {
        assert!(
            validate_meta(Path::new("/nonexistent/path"), &minimal_meta(), None)
                .is_err()
        );
    }

    #[test]
    fn rejects_empty_dir() {
        let dir = tempdir().unwrap();
        let err = validate_meta(dir.path(), &minimal_meta(), None).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn load_canonical_meta_rejects_missing_toml() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("mdrepo-metadata.toml");
        assert!(load_canonical_meta(&missing, Path::new("."), bogus_uv()).is_err());
    }

    #[test]
    fn load_canonical_meta_rejects_invalid_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mdrepo-metadata.toml");
        fs::write(&path, "not {{ valid").unwrap();
        assert!(load_canonical_meta(&path, Path::new("."), bogus_uv()).is_err());
    }

    #[test]
    fn accepts_valid_submission() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        // No ligands -> no OpenBabel spawn even though `uv` is bogus.
        let meta = load_canonical_meta(
            &dir.path().join("mdrepo-metadata.toml"),
            Path::new("."),
            bogus_uv(),
        )
        .unwrap();
        assert!(validate_meta(dir.path(), &meta, None).is_ok());
    }

    #[test]
    fn reports_file_referenced_by_metadata_but_missing() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        fs::remove_file(dir.path().join("sim.xtc")).unwrap();
        let errors = validate_meta(dir.path(), &minimal_meta(), None).unwrap();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Missing") && e.contains("sim.xtc"))
        );
    }

    #[test]
    fn reports_all_empty_files() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        for name in ["sim.xtc", "sim.pdb", "sim.top"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let errors = validate_meta(dir.path(), &minimal_meta(), None).unwrap();
        assert!(errors.iter().any(|e| e.contains("empty")));
    }

    /// Checking an upload against its manifest belongs to `ticket`, so adding
    /// one -- even a manifest every file contradicts -- changes nothing here.
    #[test]
    fn ignores_completed_json() {
        let dir = tempdir().unwrap();
        write_minimal_dir(dir.path());
        let meta = minimal_meta();
        let before = validate_meta(dir.path(), &meta, None).unwrap();

        let json = r#"{"total_filenum":1,"total_filesize":0,"status":"completed","files":[{"irods_path":"sim.xtc","size":9999,"md5_hash":"00000000000000000000000000000000"}]}"#;
        fs::write(dir.path().join("mdrepo-submission.completed.json"), json).unwrap();

        assert_eq!(validate_meta(dir.path(), &meta, None).unwrap(), before);
    }

    /// Write the minimal document plus a `[[ligands]]` block.
    fn write_dir_with_ligands(dir: &Path, ligands: &str) {
        write_minimal_dir(dir);
        fs::write(
            dir.join("mdrepo-metadata.toml"),
            format!("{MINIMAL_TOML}\n{ligands}\n"),
        )
        .unwrap();
    }

    /// A ligand declared only by InChI is skipped by canonicalization rather
    /// than mangled by it, and skipping the whole batch means no spawn at all.
    ///
    /// `bogus_uv()` is the assertion: if this tried to canonicalize, it would
    /// fail on a `uv` that cannot exist. Canonicalization is an OpenBabel round
    /// trip for SMILES and has nothing to say about an InChI string.
    #[test]
    fn an_inchi_only_ligand_is_not_sent_to_canonicalization() {
        let dir = tempdir().unwrap();
        write_dir_with_ligands(
            dir.path(),
            "[[ligands]]\nname = \"ethanol\"\ninchi = \"InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3\"",
        );

        let meta = load_canonical_meta(
            &dir.path().join("mdrepo-metadata.toml"),
            Path::new("."),
            bogus_uv(),
        )
        .expect("no SMILES to canonicalize, so no spawn");

        let ligand = &meta.ligands.as_ref().unwrap()[0];
        assert_eq!(ligand.smiles, None, "nothing may invent a SMILES here");
        assert_eq!(
            ligand.inchi.as_deref(),
            Some("InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3"),
            "the declared InChI must survive untouched"
        );
    }

    /// The off-by-one that `load_canonical_meta`'s index tuple exists to
    /// prevent, in the arrangement that would expose it: an InChI-only ligand
    /// FIRST, a SMILES ligand second.
    ///
    /// Canonicalization sends only the SMILES it found and writes the results
    /// back by original index. Lose the index and the single canonical string
    /// comes back to position 0 -- attaching one ligand's structure to another
    /// ligand's record, with no error anywhere. `validate.rs`'s count check
    /// cannot see it, exactly as `test_canonicalize_smiles.py` records for the
    /// CLI's ordering contract.
    ///
    /// Requires the real `uv` and the canonicalize script; skips otherwise, in
    /// the manner of the Postgres suites.
    #[test]
    fn canonicalization_writes_back_to_the_ligand_it_came_from() {
        let Some(uv) = which::which("uv").ok() else {
            eprintln!("skipping: no uv on PATH");
            return;
        };
        let script_dir = Path::new("../../simulation-processing/python");
        if !script_dir.join("canonicalize_smiles.py").is_file() {
            eprintln!("skipping: canonicalize_smiles.py not checked out beside us");
            return;
        }

        let dir = tempdir().unwrap();
        write_dir_with_ligands(
            dir.path(),
            "[[ligands]]\nname = \"ethanol\"\ninchi = \"InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3\"\n\
             \n[[ligands]]\nname = \"benzene\"\nsmiles = \"C1=CC=CC=C1\"",
        );

        let meta = load_canonical_meta(
            &dir.path().join("mdrepo-metadata.toml"),
            script_dir,
            &uv,
        )
        .expect("canonicalization runs");

        let ligands = meta.ligands.as_ref().unwrap();
        assert_eq!(
            ligands[0].smiles, None,
            "the InChI-only ligand gained a SMILES"
        );
        assert_eq!(ligands[0].name, "ethanol");
        assert_eq!(
            ligands[1].smiles.as_deref(),
            Some("c1ccccc1"),
            "the canonical form must land on the ligand it came from"
        );
    }
}
