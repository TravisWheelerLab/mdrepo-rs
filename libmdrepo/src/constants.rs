use lazy_regex::{Lazy, Regex, lazy_regex};
use std::collections::BTreeMap;

pub const MAX_FILE_SIZE_GB: u64 = 40;
pub const MAX_FILE_SIZE_BYTES: u64 = MAX_FILE_SIZE_GB * 10u64.pow(9);

pub static ORCID_REGEX: Lazy<Regex> = lazy_regex!(r"^\d{4}-\d{4}-\d{4}-[A-Z\d]{4}$");

pub static PDB_REGEX: Lazy<Regex> = lazy_regex!(r"^[A-Za-z0-9]{4}$");

pub static DOI_REGEX: Lazy<Regex> =
    lazy_regex!(r"^(?:https://doi.org/)?(10\.\d{4,5}\/[\S]+[^;,.\s])$");

pub static NOT_WHITESPACE_REGEX: Lazy<Regex> = lazy_regex!(r"\S+");

pub const SOLUTE_CONCENTRATION_EXCLUSIVE_MIN: f64 = 0.;
pub const SOLUTE_CONCENTRATION_EXCLUSIVE_MAX: f64 = 1.;
pub const WATER_DENSITY_MIN: f64 = 900.;
pub const WATER_DENSITY_MAX: f64 = 1100.;
pub const METADATA_TOML_VERSION: u32 = 2;
pub const TEMP_K_MIN: u32 = 275;
pub const TEMP_K_MAX: u32 = 700;
pub const TIMESTEP_FS_MIN: u32 = 1;
pub const TIMESTEP_FS_MAX: u32 = 20;
// Frame spacing, not integration timestep: the simulated time between saved
// frames. The ceiling is 100 ns per frame, well past any real sampling rate.
// These bounds only catch nonsense; they cannot catch a unit error, since a
// value entered in ns or fs still lands inside them.
//
// The floor was 0.001 ps (one femtosecond, saving every step of the finest
// timestep anyone declares) until 2026-09-08, when 1 ps became the absolute
// minimum spacing MDRepo will record from any source. Nothing in prod moved:
// only 73 simulations declare this field at all and every one of them says
// 100 ps.
pub const SAMPLING_FREQUENCY_PS_MIN: f64 = 1.;
pub const SAMPLING_FREQUENCY_PS_MAX: f64 = 100_000.;
// Below this, a spacing derived from the trajectory is not trusted on its own
// and needs the submitter to declare the same value in sampling_frequency_ps.
//
// The floor's target is a FABRICATED time axis, not a small number. A DCD or a
// solvent-stripped NetCDF carrying no usable timing gets stamped with a
// converter's default of 1 ps per frame, and that is then measured back off the
// converted file as though it had been observed. But genuine 1 ps sampling
// exists here in quantity: of the 7,202 simulations below 10 ps, 7,197 are the
// Dissociation Dynamic Database, whose paper says coordinates were saved every
// 1.0 ps. Only five are actually wrong and all five record exactly 0.
//
// No computation separates those two cases. `spacing / timestep` does not: the
// DDD's real 1 ps over its 1 fs timestep is 1,000 steps and a fabricated 1 ps
// over 4 fs is 250, both clean whole numbers. Only the submitter can say.
//
// The timestep here was 2 fs / 500 steps until 2026-09-09, which was wrong.
// Measured over the contributor's 2026-09-08 delivery, every one of the 15,525
// metadata files declares `integration_timestep_fs = 1`, and so does every one
// of the 6,664 batch-1 originals already imported. The argument is unchanged --
// both figures are still clean whole numbers -- but the number was not.
pub const SAMPLING_FLOOR_PS: f64 = 10.;
// How close a measured spacing must sit to the declared one for the
// declaration to be honoured below the floor. The declaration lowers the bar;
// it does not replace the evidence, because a declared number is the one thing
// a fabricated axis can also supply.
pub const SAMPLING_AGREEMENT_TOLERANCE: f64 = 0.01;
pub const VALID_WATER_MODEL: &[&str] = &[
    "AMOEBA",
    "BF",
    "BK3",
    "BMW",
    "COS/G2",
    "COS/G3",
    "CVFF",
    "DC",
    "ELBA",
    "EVB",
    "F3C",
    "HIPPO",
    "KKY",
    "LEWIS",
    "MARTINI polarizable water",
    "MARTINI water",
    "MB-pol",
    "MCY",
    "MS-EVB",
    "OPC",
    "OPC3",
    "OSS2",
    "POL3",
    "RWK",
    "ReaxFF",
    "SCME",
    "SDK/CMM",
    "SPC",
    "SPC/E",
    "SPC/Fd",
    "SPC/Fw",
    "ST2",
    "SWM4-NDP",
    "SWM6",
    "TIP3P",
    "TIP3P-FB",
    "TIP3P/Fs",
    "TIP4P",
    "TIP4P-CG",
    "TIP4P-D",
    "TIP4P-FB",
    "TIP4P/2005",
    "TIP4P/Ew",
    "TIP4P/Ice",
    "TIP5P",
    "TIP5P/2018",
    "TIP5P/E",
    "TIP6P",
    "TTM2-F",
    "TTM3-F",
    "TTM4-F",
    "iAMOEBA",
    "mW",
    "q-SPC/Fw",
    "q-TIP4P/F",
];

// Per-engine file format conventions, sourced from a table supplied by Ken
// (2026-08-04) and reconciled against prod (`md_uploaded_file.file_type`,
// which records the actual role -- Structure/Topology/Trajectory -- a file
// filled, not just what was uploaded). ACEMD and SPONGE have no upstream
// documentation to cite; their rows are the single combination each software
// uses 100% of the time in prod (7,459 and 565 simulations respectively,
// zero exceptions). `edr` is in GROMACS's trajectory list because 800 prod
// GROMACS simulations carry an energy file alongside `.xtc`, tagged
// Trajectory by the same convention.
//
// CUSTOM has no entry: it is the deliberate escape hatch for pipelines that
// don't follow any fixed convention, so it is exempt from combination
// checking rather than being given a fabricated set of "valid" extensions.
//
// `xtc`/`mdc` are added to every engine's trajectory list (see
// COMMON_TRAJECTORY_EXTS below) because they are conversion targets, not
// engine tells -- a mismatch on structure/topology means what was declared
// is not what was actually produced, but a trajectory alone being .xtc
// doesn't mean that.
//
// This is the single source of truth for which extensions are valid, and
// where: the old per-field STRUCTURE/TOPOLOGY/TRAJECTORY_FILE_EXTS lists were
// a hand-maintained union of this same information, which is exactly how
// ticket 1997 broke -- `rst7` existed in one list and needed to exist in
// another. See `Meta::check`'s combination check in metadata.rs.
pub struct EngineFileFormats {
    pub topology: &'static [&'static str],
    pub structure: &'static [&'static str],
    pub trajectory: Vec<&'static str>,
}

// xtc and mdc are conversion/interchange trajectory formats produced by this
// pipeline regardless of which engine ran the simulation -- e.g. MISATO's
// AMBER runs were published as .h5 and converted to .xtc on import here --
// so every engine accepts them on top of its own native trajectory formats.
const COMMON_TRAJECTORY_EXTS: &[&str] = &["xtc", "mdc"];

pub static ENGINE_FILE_FORMATS: Lazy<BTreeMap<&'static str, EngineFileFormats>> =
    Lazy::new(|| {
        let trajectory = |native: &[&'static str]| -> Vec<&'static str> {
            let mut exts: Vec<&'static str> = native.to_vec();
            for ext in COMMON_TRAJECTORY_EXTS {
                if !exts.contains(ext) {
                    exts.push(ext);
                }
            }
            exts
        };

        BTreeMap::from([
            (
                "AMBER",
                EngineFileFormats {
                    topology: &["prmtop", "parm7", "top"],
                    structure: &["inpcrd", "rst7", "restrt", "rst", "ncrst", "pdb"],
                    trajectory: trajectory(&["nc", "netcdf", "mdcrd", "crd", "trj"]),
                },
            ),
            (
                "CHARMM",
                EngineFileFormats {
                    topology: &["psf", "prm", "par", "rtf", "str"],
                    structure: &["pdb", "crd", "cor", "coor"],
                    trajectory: trajectory(&["dcd"]),
                },
            ),
            (
                "NAMD",
                EngineFileFormats {
                    topology: &["psf", "prm", "par", "rtf", "str"],
                    structure: &["pdb", "crd", "cor", "coor"],
                    trajectory: trajectory(&["dcd"]),
                },
            ),
            (
                "GROMACS",
                EngineFileFormats {
                    topology: &["top", "itp", "tpr"],
                    structure: &["gro", "pdb", "g96", "tpr"],
                    trajectory: trajectory(&["xtc", "trr", "edr"]),
                },
            ),
            (
                "ACEMD",
                EngineFileFormats {
                    topology: &["psf"],
                    structure: &["pdb"],
                    trajectory: trajectory(&["xtc"]),
                },
            ),
            (
                "SPONGE",
                EngineFileFormats {
                    topology: &["gro"],
                    structure: &["pdb"],
                    trajectory: trajectory(&["xtc"]),
                },
            ),
        ])
    });

// The per-field "is this extension recognized by any engine at all" lists,
// derived from `ENGINE_FILE_FORMATS` rather than hand-maintained, so an
// extension can't exist in one and be missing from another.
fn union_exts<'a>(
    select: impl Fn(&'a EngineFileFormats) -> &'a [&'static str],
) -> Vec<&'static str> {
    let mut exts: Vec<&'static str> = ENGINE_FILE_FORMATS
        .values()
        .flat_map(|fmt| select(fmt).iter().copied())
        .collect();
    exts.sort_unstable();
    exts.dedup();
    exts
}

pub static TRAJECTORY_FILE_EXTS: Lazy<Vec<&'static str>> =
    Lazy::new(|| union_exts(|fmt| fmt.trajectory.as_slice()));

pub static STRUCTURE_FILE_EXTS: Lazy<Vec<&'static str>> =
    Lazy::new(|| union_exts(|fmt| fmt.structure));

pub static TOPOLOGY_FILE_EXTS: Lazy<Vec<&'static str>> =
    Lazy::new(|| union_exts(|fmt| fmt.topology));

const ACEMD_VERSIONS: &[&str] = &[
    "4.0.20", "4.0.18", "4.0.17", "4.0.16", "4.0.15", "4.0.11", "4.0.9", "4.0.1",
    "4.0.0", "3.7.3", "3.7.2", "3.7.1", "3.7.0", "3.6.0", "3.5.1", "3.5.0", "3.4.1",
    "3.4.0", "3.3.0", "3.2.4", "3.2.3", "3.2.2", "3.2.1", "3.2.0", "3.1.2", "3.1.1",
    "3.1.0", "3.0.4", "3.0.3", "3.0.2", "3.0.1", "3.0.0",
];

const GROMACS_VERSIONS: &[&str] = &[
    "2016", "2016.1", "2016.2", "2016.3", "2016.4", "2016.5", "2016.6", "2018",
    "2018.1", "2018.2", "2018.3", "2018.4", "2018.5", "2018.6", "2018.7", "2018.8",
    "2019", "2019.1", "2019.2", "2019.3", "2019.4", "2019.5", "2019.6", "2020",
    "2020.1", "2020.2", "2020.3", "2020.4", "2020.5", "2020.6", "2020.7", "2021",
    "2021.1", "2021.2", "2021.3", "2021.4", "2021.5", "2021.6", "2021.7", "2022",
    "2022.1", "2022.2", "2022.3", "2022.4", "2022.5", "2022.6", "2023", "2023.1",
    "2023.2", "2023.3", "2023.4", "2023.5", "2024", "2024.1", "2024.2", "2024.3",
    // GROMACS names the first release of a series with no patch component --
    // 2016, 2018, ... 2023, 2024 above all follow that, and there is no
    // "2024.0". 2025 was the one series entered as "2025.0" instead, so the
    // real release was refused: simulation 20615 declares a bare "2025" and is
    // one of the invalid released TOMLs solely because of this line.
    //
    // "2025.0" and "2026.0" are kept although GROMACS ships neither. Removing
    // them would be a tightening nobody asked for, and rejecting a submitter
    // who writes the .0 they reasonably expect to exist is the failure mode
    // this fix exists to undo. Measured 2026-09-09: prod holds 1 simulation at
    // "2025" and 15,516 at "2025.3", and none at either .0 spelling.
    "2024.4", "2024.5", "2024.6", "2025", "2025.0", "2025.1", "2025.2", "2025.3",
    "2025.4", "2026", "2026.0", "2026.1", "2026.2", "2026.3", "3.0", "3.0.1", "3.0.2",
    "3.0.3", "3.0.4", "3.0.5", "3.1", "3.1.1", "3.1.2", "3.1.3", "3.1.4", "3.2",
    "3.2.1", "3.3", "3.3.1", "3.3.2", "3.3.3", "3.3.4", "4.0", "4.0.2", "4.0.3",
    "4.0.4", "4.0.5", "4.0.6", "4.0.7", "4.5", "4.5.1", "4.5.2", "4.5.3", "4.5.4",
    "4.5.5", "4.5.6", "4.5.7", "4.6", "4.6.1", "4.6.2", "4.6.3", "4.6.4", "4.6.5",
    "4.6.6", "4.6.7", "5.0", "5.0.1", "5.0.2", "5.0.3", "5.0.4", "5.0.5", "5.0.6",
    "5.0.7", "5.1", "5.1.1", "5.1.2", "5.1.3", "5.1.4", "5.1.5",
];

const AMBER_VERSIONS: &[&str] = &[
    "9", "10", "11", "2012", "2014", "2016", "2018", "2020", "2022", "2024",
];

const NAMD_VERSIONS: &[&str] = &[
    "2.6", "2.7", "2.8", "2.9", "2.10", "2.11", "2.12", "2.13", "2.14", "3.0", "3.0.1",
];

const CHARMM_VERSIONS: &[&str] = &[
    "27", "28", "29", "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "40",
    "41", "42", "43", "44", "45", "46", "47", "48", "49", "50",
];

const SPONGE_VERSIONS: &[&str] = &["1.1", "1.2", "1.3", "1.4"];

const CUSTOM_VERSIONS: &[&str] = &["NA"];

pub const VALID_SOLUTE_NAME: &[&str] = &[
    "Cl-",
    "Cl",
    "K",
    "K+",
    "Na",
    "Na+",
    "Phosphoric acid",
    "Urea",
];

pub static VALID_SOFTWARE: Lazy<BTreeMap<&'static str, &'static [&'static str]>> =
    Lazy::new(|| {
        BTreeMap::from([
            ("ACEMD", ACEMD_VERSIONS),
            ("AMBER", AMBER_VERSIONS),
            ("CHARMM", CHARMM_VERSIONS),
            ("CUSTOM", CUSTOM_VERSIONS),
            ("GROMACS", GROMACS_VERSIONS),
            ("NAMD", NAMD_VERSIONS),
            ("SPONGE", SPONGE_VERSIONS),
        ])
    });
