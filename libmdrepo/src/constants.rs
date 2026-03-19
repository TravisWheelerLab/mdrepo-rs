use regex::Regex;
use std::sync::LazyLock;

pub const MAX_FILE_SIZE_GB: u64 = 40;
pub const MAX_FILE_SIZE_BYTES: u64 = MAX_FILE_SIZE_GB * 10u64.pow(9);

pub static ORCID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{4}-\d{4}-[A-Z\d]{4}$").unwrap());

pub static PDB_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9]{4}$").unwrap());

pub static DOI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:https://doi.org/)?(10\.\d{4,5}\/[\S]+[^;,.\s])$").unwrap()
});

pub static NOT_WHITESPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\S+").unwrap());

pub const SOLVENT_CONCENTRATION_MIN: f64 = 0.;
pub const SOLVENT_CONCENTRATION_MAX: f64 = 1.;
pub const WATER_DENSITY_MIN: f64 = 900.;
pub const WATER_DENSITY_MAX: f64 = 1100.;
pub const METADATA_TOML_VERSION: u32 = 2;
pub const TEMP_K_MIN: u32 = 275;
pub const TEMP_K_MAX: u32 = 700;
pub const TIMESTEP_FS_MIN: u32 = 1;
pub const TIMESTEP_FS_MAX: u32 = 5;
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
    "iAMOEBA",
    "KKY",
    "LEWIS",
    "MARTINI polarizable water",
    "MARTINI water",
    "MB-pol",
    "MCY",
    "MS-EVB",
    "mW",
    "OPC",
    "OPC3",
    "OSS2",
    "POL3",
    "q-SPC/Fw",
    "q-TIP4P/F",
    "ReaxFF",
    "RWK",
    "SCME",
    "SDK/CMM",
    "SPC",
    "SPC/E",
    "SPC/Fd",
    "SPC/Fw",
    "SPC/Fw",
    "ST2",
    "SWM4-NDP",
    "SWM6",
    "TIP3P-FB",
    "TIP3P",
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
];
