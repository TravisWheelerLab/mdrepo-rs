use lazy_regex::{Lazy, Regex, lazy_regex};
use std::collections::HashMap;

pub const MAX_FILE_SIZE_GB: u64 = 40;
pub const MAX_FILE_SIZE_BYTES: u64 = MAX_FILE_SIZE_GB * 10u64.pow(9);

pub static ORCID_REGEX: Lazy<Regex> = lazy_regex!(r"^\d{4}-\d{4}-\d{4}-[A-Z\d]{4}$");

pub static PDB_REGEX: Lazy<Regex> = lazy_regex!(r"^[A-Za-z0-9]{4}$");

pub static DOI_REGEX: Lazy<Regex> =
    lazy_regex!(r"^(?:https://doi.org/)?(10\.\d{4,5}\/[\S]+[^;,.\s])$");

pub static NOT_WHITESPACE_REGEX: Lazy<Regex> = lazy_regex!(r"\S+");

pub static MOLLY_TIME_REGEX: Lazy<Regex> =
    lazy_regex!(r"^time:\s*(\d+)-(\d+(?:\.\d)?)\s+ps");

pub static MOLLY_NFRAMES_REGEX: Lazy<Regex> = lazy_regex!(r"^nframes:\s*(\d+)");

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

// Here are common trajectory file extensions used in molecular dynamics simulations:
// GROMACS
// - .xtc — compressed trajectory (positions only)
// - .trr — full-precision trajectory (positions, velocities, forces)
// - .tng — trajectory next generation format
// - .edr — energy trajectory
// AMBER
// - .nc / .ncdf — NetCDF trajectory
// - .mdcrd / .crd — ASCII coordinate trajectory
// - .dcd — DCD binary trajectory (also used by NAMD/CHARMM)
// - .rst / .rst7 — restart/checkpoint file
// NAMD / CHARMM
// - .dcd — DCD binary trajectory
// - .coor — coordinate file
// - .vel — velocity file
// LAMMPS
// - .lammpstrj — LAMMPS dump trajectory
// - .dump — generic dump format
// OpenMM
// - .dcd — DCD binary trajectory
// - .pdb — PDB trajectory (multi-model)
// Desmond / Schrödinger
// - .dtr — Desmond trajectory directory format
// - .xtc — also supported
// General / Multi-software
// - .pdb — multi-model PDB
// - .xyz — multi-frame XYZ
// - .h5 / .hdf5 — HDF5-based trajectories (MDTraj, etc.)
// - .mol2 — multi-model Mol2
// - .trj — generic trajectory (various tools)
// - .binpos — binary positions (AMBER)
pub const TRAJECTORY_FILE_EXTS: &[&str] = &[
    "xtc", "nc", "ncdf", "dcd", "trr", "tng", "edr", "rst", "rst7", "coor", "vel",
    "trj",
];

// Here are common structure file extensions used in molecular dynamics simulations:
// Universal / Multi-software
// - .pdb — Protein Data Bank format (most widely used)
// - .cif / .mmcif — macromolecular crystallographic information file
// - .mol — MDL Molfile (small molecules)
// - .mol2 — Tripos Mol2 (atoms, bonds, charges)
// - .xyz — simple Cartesian coordinate format
// - .sdf — structure data file (small molecules, ligands)
// GROMACS
// - .gro — GROMACS coordinate file (positions + box vectors)
// AMBER
// - .inpcrd / .crd — input coordinate file
// - .rst / .rst7 — restart file (coordinates + velocities)
// - .ncrst — NetCDF restart file
// NAMD / CHARMM
// - .crd — CHARMM coordinate file
// - .cor — CHARMM coordinate file (alternate extension)
// - .dms — DESRES molecular structure file
// Desmond / Schrödinger
// - .mae / .maegz — Maestro structure file (compressed)
// - .cms — composite model system
// LAMMPS
// - .data — LAMMPS data file (includes coordinates)
// - .dump — LAMMPS dump snapshot
// OpenBabel / Cheminformatics
// - .smi / .smiles — SMILES string
// - .inchi — IUPAC International Chemical Identifier
// - .pqr — PDB with charge and radius (used in PB/SA calculations)
// Crystallography / Periodic Systems
// - .cif — crystallographic information file
// - .vasp / POSCAR — VASP structure format
// - .config / .cfg — DL_POLY config file
// - .lmp — LAMMPS structure (alternate extension)
pub const STRUCTURE_FILE_EXTS: &[&str] = &["pdb", "gro"];

// Here are common topology file extensions used in molecular dynamics simulations:
// GROMACS
// - .top — main topology file
// - .itp — include topology (force field parameters, molecule definitions)
// - .tpr — portable binary run input (compiled topology + parameters)
// AMBER
// - .prmtop / .parm7 — parameter/topology file
// - .parm — older AMBER topology format
// NAMD / CHARMM
// - .psf — protein structure file (bonds, angles, atom types)
// - .rtf / .resi — residue topology file
// - .prm / .par — parameter file
// LAMMPS
// - .data — LAMMPS data file (topology + force field)
// - .in — input script (often contains topology info)
// OpenMM / ParmEd
// - .xml — OpenMM system XML file
// - .prmtop — AMBER topology (widely supported)
// Desmond / Schrödinger
// - .cms — composite model system (topology + coordinates)
// - .msj — multisim job file
// General / Structure Files (often double as topology)
// - .pdb — Protein Data Bank format
// - .gro — GROMACS structure file (coordinates + box)
// - .mol2 — Tripos Mol2 (bonds + atom types)
// - .cif / .mmcif — crystallographic information file
// - .mae / .maegz — Maestro format (Schrödinger)
// Force Field Related
// - .xml — OpenMM force field definitions
// - .frcmod — AMBER force field modification file
// - .str — CHARMM stream file (partial charges, parameters)
// - .ff — generic force field file
pub const TOPOLOGY_FILE_EXTS: &[&str] =
    &["top", "gro", "psf", "prmtop", "parm7", "prm", "rtf", "tpr"];

const GROMACS_VERSIONS: &[&str] = &[
    "3.3", "3.3.3", "4.0", "4.0.7", "4.5", "4.5.7", "4.6", "4.6.7", "5.0", "5.0.7",
    "5.1", "5.1.5", "2016", "2016.1", "2016.2", "2016.3", "2016.4", "2016.5", "2016.6",
    "2018", "2018.1", "2018.2", "2018.3", "2018.4", "2018.5", "2018.6", "2018.7",
    "2018.8", "2019", "2019.1", "2019.2", "2019.3", "2019.4", "2019.5", "2019.6",
    "2020", "2020.1", "2020.2", "2020.3", "2020.4", "2020.5", "2020.6", "2020.7",
    "2021", "2021.1", "2021.2", "2021.3", "2021.4", "2021.5", "2021.6", "2021.7",
    "2022", "2022.1", "2022.2", "2022.3", "2022.4", "2022.5", "2022.6", "2023",
    "2023.1", "2023.2", "2023.3", "2023.4", "2023.5", "2024", "2024.1", "2024.2",
    "2024.3", "2024.4",
];

const AMBER_VERSIONS: &[&str] = &[
    "1", "2", "3", "4", "4.1", "5", "6", "7", "8", "9", "10", "11", "12", "14", "16",
    "18", "20", "22", "24",
];

const NAMD_VERSIONS: &[&str] = &[
    "2.6", "2.7", "2.8", "2.9", "2.10", "2.11", "2.12", "2.13", "2.14", "3.0", "3.0.1",
];

const CHARMM_VERSIONS: &[&str] = &[
    "c27", "c28", "c29", "c30", "c31", "c32", "c33", "c34", "c35", "c36", "c37", "c38",
    "c39", "c40", "c41", "c42", "c43", "c44", "c45", "c46", "c47", "c48",
];

pub static VALID_SOFTWARE: Lazy<HashMap<&'static str, &'static [&'static str]>> =
    Lazy::new(|| {
        HashMap::from([
            ("GROMACS", GROMACS_VERSIONS),
            ("AMBER", AMBER_VERSIONS),
            ("NAMD", NAMD_VERSIONS),
            ("CHARMM", CHARMM_VERSIONS),
        ])
    });
