//! Check that a simulation's processed files agree with each other.
//!
//! WHY THIS EXISTS. The checks we had looked at each file alone -- atom
//! counts, coordinate ranges, a .gro box line being present -- and at storage
//! (size, checksum). Nothing read what was inside a trajectory beyond that.
//! So a box-less run whose XTC frames carried an uninitialized box ([inf,
//! -inf, ...], or -7083.2 nm, or -6.9e14 nm) passed everything, for years:
//! cpptraj writes whatever memory holds when told nothing about the box, and
//! an old-vs-new byte comparison could not tell that from noise. Found
//! 2026-11-09 on MDR00105222 (simulation-processing #44).
//!
//! Three checks, each returning [`Finding`]s:
//!
//! - **box**: every frame's box is finite and either zero in every frame or
//!   a real cell in every frame (three vectors 0.1-10,000 nm enclosing a
//!   volume); it matches the pipeline's own box decision when given;
//!   full.pdb's CRYST1 agrees with full.xtc's first frame. Lengths and angles
//!   are compared, not the matrix: the RMS fit rotates the box.
//! - **time**: frame times are finite and never go backwards (fail); a
//!   repeated time (a restart joined with its boundary frame twice) or no
//!   time axis at all (released 2025 sampled.xtc: every frame at 0 ps) warn;
//!   uneven spacing is reported only, since some simulations skip time.
//!   full.xtc and minimal.xtc have the same frames. Given an XTC source, its
//!   times must match full.xtc's: the conversion can stamp an even grid over
//!   a source that skipped time.
//! - **atoms**: every file of a set (full: psf, pdb, mmcif, xtc; minimal: the
//!   same plus sampled.xtc) has the same atoms in the same order as the .psf,
//!   the topology, with the same names and residue names, split into the
//!   same residues (numbering may differ -- .psf restarts it per segment,
//!   cpptraj's .pdb does not -- reported as info); elements are real element
//!   symbols; .pdb and .mmcif coordinates equal the trajectory's first frame
//!   within the formats' precision. .gro files are not checked: they are to
//!   be deprecated, and full.gro is the submitter's structure, not frame 0.
//!
//! Read-only. A missing file is skipped, so the same checks run on a fresh
//! `process` output (no .psf/.mmcif yet), on an MDR-108 build (orig/ + new/,
//! no full.xtc), or on a release fetched from IRODS. The rules were tuned on
//! real outputs (25 processed folders, 30 MDR-108 builds) on 2026-11-09,
//! first as simulation-processing's check_outputs.py (#47).

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
};

/// XTC stores nm to 3 decimals by default (0.01 A steps) and PDB A to 3, so
/// the worst honest disagreement is about 0.01 A; 0.02 A leaves room.
pub const COORD_TOL_A: f64 = 0.02;
const BOX_LEN_RTOL: f64 = 2e-3;
const BOX_ANGLE_TOL_DEG: f64 = 0.2;
const ZERO: f64 = 1e-6;

const ELEMENTS: &str = "H He Li Be B C N O F Ne Na Mg Al Si P S Cl Ar K Ca Sc \
    Ti V Cr Mn Fe Co Ni Cu Zn Ga Ge As Se Br Kr Rb Sr Y Zr Nb Mo Tc Ru Rh Pd \
    Ag Cd In Sn Sb Te I Xe Cs Ba La Ce Pr Nd Pm Sm Eu Gd Tb Dy Ho Er Tm Yb Lu \
    Hf Ta W Re Os Ir Pt Au Hg Tl Pb Bi Po At Rn Fr Ra Ac Th Pa U Np Pu Am Cm \
    Bk Cf Es Fm Md No Lr Rf Db Sg Bh Hs Mt Ds Rg Cn Nh Fl Mc Lv Ts Og D";

type Mat3 = [[f64; 3]; 3];
type Xyz = [f64; 3];

// --------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Fail,
    Warn,
    Info,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Fail => "FAIL",
            Level::Warn => "WARN",
            Level::Info => "INFO",
        })
    }
}

/// One result of one check
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub check: &'static str,
    pub level: Level,
    pub file: String,
    pub message: String,
}

fn finding(check: &'static str, level: Level, file: &str, message: String) -> Finding {
    Finding {
        check,
        level,
        file: file.to_string(),
        message,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub name: String,
    pub resname: String,
    pub resid: i64,
    pub element: String,
}

/// A structure or topology file's atoms; coordinates in angstroms, or None
#[derive(Debug, Clone)]
pub struct Structure {
    pub name: String,
    pub atoms: Vec<Atom>,
    pub xyz: Option<Vec<Xyz>>,
    /// Residue numbers wrap at this (the .pdb's 4 columns)
    pub resid_wrap: Option<i64>,
    /// The widest residue and atom names the format can hold
    pub resname_width: Option<usize>,
    pub name_width: Option<usize>,
}

/// What the checks need from a trajectory, read once
#[derive(Debug, Clone)]
pub struct Frames {
    pub name: String,
    pub natoms: usize,
    pub times: Vec<f64>,
    /// One 3x3 box per frame, nm, rows the box vectors
    pub boxes: Vec<Mat3>,
    /// The first frame's coordinates, angstroms
    pub first_xyz: Option<Vec<Xyz>>,
}

// --- readers ---------------------------------------------------------------
fn base_name(path: &Path) -> String {
    path.file_name().map_or(path.display().to_string(), |n| {
        n.to_string_lossy().to_string()
    })
}

/// Times, boxes and the first frame of an XTC.
///
/// molly asserts on a frame header whose two atom counts disagree, which on
/// a damaged file would panic; that is caught here and reported as an
/// unreadable file like any other read error.
pub fn read_xtc(path: &Path) -> Result<Frames> {
    let path_buf = path.to_path_buf();
    let read = std::panic::catch_unwind(move || -> Result<Frames> {
        let mut reader = molly::XTCReader::open(&path_buf)?;
        let mut frames = Frames {
            name: base_name(&path_buf),
            natoms: 0,
            times: vec![],
            boxes: vec![],
            first_xyz: None,
        };
        loop {
            let mut frame = molly::Frame::default();
            match reader.read_frame(&mut frame) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let b = frame.boxvec_cols_2d();
            frames.boxes.push(b.map(|row| row.map(f64::from)));
            frames.times.push(f64::from(frame.time));
            if frames.first_xyz.is_none() {
                frames.natoms = frame.natoms();
                frames.first_xyz = Some(
                    frame
                        .coords()
                        .map(|p| p.map(|v| f64::from(v) * 10.0))
                        .collect(),
                );
            }
        }
        Ok(frames)
    });
    match read {
        Ok(r) => r,
        Err(_) => bail!(
            "{}: the XTC reader stopped on a damaged frame",
            path.display()
        ),
    }
}

fn int(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

fn float(s: &str) -> Result<f64> {
    s.trim()
        .parse()
        .map_err(|e| anyhow!("not a number {:?}: {e}", s.trim()))
}

/// Columns `a..b` of a line, as far as the line reaches
fn cols(line: &str, a: usize, b: usize) -> &str {
    let n = line.len();
    if a >= n {
        ""
    } else {
        line.get(a..b.min(n)).unwrap_or("")
    }
}

/// ATOM/HETATM records of the first model. cpptraj writes a 4-letter
/// residue name from column 17 (TIP3), so the name is read from there.
pub fn read_pdb(path: &Path) -> Result<Structure> {
    let text = fs::read_to_string(path)?;
    let (mut atoms, mut xyz) = (vec![], vec![]);
    for line in text.lines() {
        if line.starts_with("ATOM  ") || line.starts_with("HETATM") {
            atoms.push(Atom {
                name: cols(line, 12, 16).trim().to_string(),
                resname: cols(line, 16, 21).trim().to_string(),
                resid: int(cols(line, 22, 26)),
                element: cols(line, 76, 78).trim().to_string(),
            });
            xyz.push([
                float(cols(line, 30, 38))?,
                float(cols(line, 38, 46))?,
                float(cols(line, 46, 54))?,
            ]);
        } else if line.starts_with("ENDMDL") {
            break;
        }
    }
    Ok(Structure {
        name: base_name(path),
        atoms,
        xyz: Some(xyz),
        resid_wrap: Some(10000),
        resname_width: Some(4),
        name_width: Some(4),
    })
}

/// CRYST1 a b c alpha beta gamma, or None if there is none before the atoms
pub fn read_pdb_cryst1(path: &Path) -> Result<Option<[f64; 6]>> {
    for line in fs::read_to_string(path)?.lines() {
        if line.starts_with("CRYST1") {
            let spans = [(6, 15), (15, 24), (24, 33), (33, 40), (40, 47), (47, 54)];
            let mut c = [0.0; 6];
            for (v, (a, b)) in c.iter_mut().zip(spans) {
                *v = float(cols(line, a, b))?;
            }
            return Ok(Some(c));
        }
        if line.starts_with("ATOM  ") || line.starts_with("HETATM") {
            return Ok(None);
        }
    }
    Ok(None)
}

/// The !NATOM section: id segid resid resname name type charge mass
pub fn read_psf(path: &Path) -> Result<Structure> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    let mut atoms = vec![];
    while let Some(line) = lines.next() {
        if line.contains("!NATOM") {
            let n: usize = line
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| anyhow!("unreadable !NATOM line {line:?}"))?;
            for _ in 0..n {
                let f: Vec<&str> = lines
                    .next()
                    .ok_or_else(|| anyhow!("fewer than {n} atom lines"))?
                    .split_whitespace()
                    .collect();
                if f.len() < 5 {
                    bail!("short atom line: {}", f.join(" "));
                }
                atoms.push(Atom {
                    name: f[4].to_string(),
                    resname: f[3].to_string(),
                    resid: int(f[2]),
                    element: String::new(),
                });
            }
            break;
        }
    }
    Ok(Structure {
        name: base_name(path),
        atoms,
        xyz: None,
        resid_wrap: None,
        resname_width: None,
        name_width: None,
    })
}

/// Whitespace-separated mmCIF values; '...' and "..." may hold spaces and
/// end at a quote followed by whitespace or the end of the line.
fn cif_tokens(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let (mut out, mut i) = (vec![], 0);
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let q = chars[i];
        if q == '\'' || q == '"' {
            let mut j = i + 1;
            while j < chars.len()
                && !(chars[j] == q
                    && (j + 1 == chars.len() || chars[j + 1].is_whitespace()))
            {
                j += 1;
            }
            out.push(chars[i + 1..j.min(chars.len())].iter().collect());
            i = j + 1;
        } else {
            let mut j = i;
            while j < chars.len() && !chars[j].is_whitespace() {
                j += 1;
            }
            out.push(chars[i..j].iter().collect());
            i = j;
        }
    }
    out
}

/// _atom_site, first model
pub fn read_mmcif(path: &Path) -> Result<Structure> {
    let text = fs::read_to_string(path)?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with("_atom_site."))
        .ok_or_else(|| anyhow!("no _atom_site table"))?;
    let mut names = vec![];
    let mut i = start;
    while i < lines.len() && lines[i].trim_start().starts_with("_atom_site.") {
        names.push(lines[i].trim()["_atom_site.".len()..].to_string());
        i += 1;
    }
    let col =
        |keys: &[&str]| keys.iter().find_map(|k| names.iter().position(|n| n == k));
    let c_name = col(&["auth_atom_id", "label_atom_id"])
        .ok_or_else(|| anyhow!("no atom names"))?;
    let c_comp = col(&["auth_comp_id", "label_comp_id"])
        .ok_or_else(|| anyhow!("no residue names"))?;
    let c_seq = col(&["auth_seq_id", "label_seq_id"])
        .ok_or_else(|| anyhow!("no residue numbers"))?;
    let c_el = col(&["type_symbol"]);
    let c_model = col(&["pdbx_PDB_model_num"]);
    let cx = col(&["Cartn_x"]).ok_or_else(|| anyhow!("no Cartn_x"))?;
    let cy = col(&["Cartn_y"]).ok_or_else(|| anyhow!("no Cartn_y"))?;
    let cz = col(&["Cartn_z"]).ok_or_else(|| anyhow!("no Cartn_z"))?;

    let (mut atoms, mut xyz, mut first_model) = (vec![], vec![], None::<String>);
    let mut pending: Vec<String> = vec![];
    for line in &lines[i..] {
        let t = line.trim();
        if t.is_empty()
            || t.starts_with('#')
            || t.starts_with('_')
            || t.starts_with("loop_")
            || t.starts_with("data_")
        {
            break;
        }
        pending.extend(cif_tokens(line));
        while pending.len() >= names.len() {
            let row: Vec<String> = pending.drain(..names.len()).collect();
            if let Some(m) = c_model {
                match &first_model {
                    None => first_model = Some(row[m].clone()),
                    Some(fm) if *fm != row[m] => return finish_mmcif(path, atoms, xyz),
                    _ => {}
                }
            }
            let el = c_el.map_or("", |c| row[c].as_str());
            atoms.push(Atom {
                name: row[c_name].clone(),
                resname: row[c_comp].clone(),
                resid: int(&row[c_seq]),
                element: if el == "?" || el == "." {
                    String::new()
                } else {
                    el.to_string()
                },
            });
            xyz.push([float(&row[cx])?, float(&row[cy])?, float(&row[cz])?]);
        }
    }
    if !pending.is_empty() {
        bail!("the _atom_site table ends part way through a row");
    }
    finish_mmcif(path, atoms, xyz)
}

fn finish_mmcif(path: &Path, atoms: Vec<Atom>, xyz: Vec<Xyz>) -> Result<Structure> {
    Ok(Structure {
        name: base_name(path),
        atoms,
        xyz: Some(xyz),
        resid_wrap: None,
        resname_width: None,
        name_width: None,
    })
}

/// Stored frame times of an XTC source; None for any other format (a DCD
/// carries only a header spacing, so it cannot have skipped time).
pub fn read_source_times(path: &Path) -> Result<Option<Vec<f64>>> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != "xtc" {
        return Ok(None);
    }
    Ok(Some(read_xtc(path)?.times))
}

// --- box geometry ------------------------------------------------------------
fn norm(v: &Xyz) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn dot(a: &Xyz, b: &Xyz) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn det(m: &Mat3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// zero, cell, bad or non-finite, for one 3x3 box in nm
pub fn box_kind(b: &Mat3) -> &'static str {
    if b.iter().flatten().any(|v| !v.is_finite()) {
        return "non-finite";
    }
    if b.iter().flatten().all(|v| v.abs() < ZERO) {
        return "zero";
    }
    let lens = b.map(|r| norm(&r));
    let (lo, hi) = lens
        .iter()
        .fold((f64::MAX, 0.0f64), |(lo, hi), &l| (lo.min(l), hi.max(l)));
    if lo < 0.1 || hi > 1e4 {
        return "bad";
    }
    if det(b).abs() < 0.1 * lens[0] * lens[1] * lens[2] {
        return "bad"; // nearly flat: not a cell
    }
    "cell"
}

/// Vector lengths (nm) and alpha, beta, gamma (degrees)
pub fn lengths_angles(b: &Mat3) -> ([f64; 3], [f64; 3]) {
    let l = b.map(|r| norm(&r));
    let ang = |u: &Xyz, v: &Xyz, lu: f64, lv: f64| {
        (dot(u, v) / (lu * lv)).clamp(-1.0, 1.0).acos().to_degrees()
    };
    (
        l,
        [
            ang(&b[1], &b[2], l[1], l[2]),
            ang(&b[0], &b[2], l[0], l[2]),
            ang(&b[0], &b[1], l[0], l[1]),
        ],
    )
}

/// 3x3 box (nm) from CRYST1 a b c (A) alpha beta gamma (degrees)
pub fn box_from_cryst1(c: &[f64; 6]) -> Mat3 {
    let (a, b, cc) = (c[0] / 10.0, c[1] / 10.0, c[2] / 10.0);
    let (al, be, ga) = (c[3].to_radians(), c[4].to_radians(), c[5].to_radians());
    let cx = cc * be.cos();
    let cy = cc * (al.cos() - be.cos() * ga.cos()) / ga.sin();
    [
        [a, 0.0, 0.0],
        [b * ga.cos(), b * ga.sin(), 0.0],
        [cx, cy, (cc * cc - cx * cx - cy * cy).max(0.0).sqrt()],
    ]
}

pub fn same_cell(b1: &Mat3, b2: &Mat3) -> bool {
    let ((l1, a1), (l2, a2)) = (lengths_angles(b1), lengths_angles(b2));
    (0..3).all(|i| {
        (l1[i] - l2[i]).abs() <= BOX_LEN_RTOL * l1[i].max(l2[i])
            && (a1[i] - a2[i]).abs() <= BOX_ANGLE_TOL_DEG
    })
}

fn fmt_box(b: &Mat3) -> String {
    let v: Vec<String> = b
        .iter()
        .flatten()
        .map(|x| format!("{}", fmt_g(*x)))
        .collect();
    format!("[{}]", v.join(", "))
}

/// A short number: 4 significant figures, "inf" and "-inf" as such
fn fmt_g(x: f64) -> String {
    if !x.is_finite() || x == 0.0 {
        return format!("{x}");
    }
    let mag = x.abs().log10().floor() as i32;
    if !(-4..6).contains(&mag) {
        format!("{x:.3e}")
    } else {
        let dp = (3 - mag).max(0) as usize;
        let s = format!("{x:.dp$}");
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    }
}

// --- check 1: box --------------------------------------------------------------
pub fn check_box(
    trajs: &[Frames],
    files: &HashMap<String, PathBuf>,
    box_present: Option<bool>,
) -> Result<Vec<Finding>> {
    let mut out = vec![];
    let mut summary = vec![];
    for fr in trajs {
        let kinds: Vec<&str> = fr.boxes.iter().map(box_kind).collect();
        let count = |k: &str| kinds.iter().filter(|&&x| x == k).count();
        let present: Vec<&str> = ["zero", "cell", "bad", "non-finite"]
            .into_iter()
            .filter(|k| count(k) > 0)
            .collect();
        summary.push(format!(
            "{}: {}",
            fr.name,
            present
                .iter()
                .map(|k| format!("{k} {}", count(k)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for k in ["non-finite", "bad"] {
            if let Some(i) = kinds.iter().position(|&x| x == k) {
                out.push(finding(
                    "box",
                    Level::Fail,
                    &fr.name,
                    format!(
                        "{} of {} frames have a {k} box (first at frame {i}: {})",
                        count(k),
                        kinds.len(),
                        fmt_box(&fr.boxes[i])
                    ),
                ));
            }
        }
        let (zeros, cells) = (count("zero"), count("cell"));
        if zeros > 0 && cells > 0 {
            out.push(finding(
                "box",
                Level::Fail,
                &fr.name,
                format!("{zeros} frames have no box and {cells} have one"),
            ));
        }
        if let Some(bp) = box_present
            && count("bad") + count("non-finite") == 0
        {
            if bp && zeros > 0 && fr.name != "sampled.xtc" {
                out.push(finding(
                    "box",
                    Level::Fail,
                    &fr.name,
                    "the run has a box but these frames have none".into(),
                ));
            }
            if !bp && cells > 0 {
                out.push(finding(
                    "box",
                    Level::Fail,
                    &fr.name,
                    "the run has no box but these frames have one".into(),
                ));
            }
        }
    }

    let full = trajs.iter().find(|f| f.name == "full.xtc");
    if let (Some(fx), Some(pdb)) = (full, files.get("full.pdb"))
        && let Some(first) = fx.boxes.first()
    {
        let cryst1 = read_pdb_cryst1(pdb)?;
        if box_kind(first) == "cell" {
            match cryst1 {
                None => out.push(finding(
                    "box",
                    Level::Warn,
                    "full.pdb",
                    "no CRYST1 although full.xtc has a box".into(),
                )),
                Some(c) if !same_cell(&box_from_cryst1(&c), first) => {
                    out.push(finding(
                        "box",
                        Level::Fail,
                        "full.pdb",
                        format!(
                            "CRYST1 {c:?} differs from full.xtc frame 0 {}",
                            fmt_box(first)
                        ),
                    ))
                }
                _ => {}
            }
        } else if let Some(c) = cryst1 {
            // cpptraj's "no box" is a 1 A cube or zero lengths
            let no_box = (c[0] == 1.0 && c[1] == 1.0 && c[2] == 1.0)
                || c[..3].iter().all(|v| *v == 0.0);
            if !no_box && box_kind(&box_from_cryst1(&c)) != "zero" {
                out.push(finding(
                    "box",
                    Level::Fail,
                    "full.pdb",
                    format!("CRYST1 {c:?} although full.xtc has no box"),
                ));
            }
        }
    }
    if out.is_empty() {
        let msg = if summary.is_empty() {
            "no trajectory to check".into()
        } else {
            summary.join("; ")
        };
        out.push(finding("box", Level::Info, "", msg));
    }
    Ok(out)
}

// --- check 2: time ---------------------------------------------------------------
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

pub fn check_time(
    trajs: &[Frames],
    source_times: Option<&[f64]>,
    source_name: &str,
) -> Vec<Finding> {
    let mut out = vec![];
    for fr in trajs {
        let t = &fr.times;
        let bad = t.iter().filter(|x| !x.is_finite()).count();
        if bad > 0 {
            out.push(finding(
                "time",
                Level::Fail,
                &fr.name,
                format!("{bad} frame times are not finite"),
            ));
            continue;
        }
        if t.len() > 1 && t.iter().all(|x| *x == t[0]) {
            out.push(finding(
                "time",
                Level::Warn,
                &fr.name,
                format!(
                    "no time axis: all {} frames are at {} ps",
                    t.len(),
                    fmt_g(t[0])
                ),
            ));
            continue;
        }
        let d: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
        let back: Vec<usize> = (0..d.len()).filter(|&i| d[i] < 0.0).collect();
        let same: Vec<usize> = (0..d.len()).filter(|&i| d[i] == 0.0).collect();
        if let Some(&i) = back.first() {
            out.push(finding(
                "time",
                Level::Fail,
                &fr.name,
                format!(
                    "time goes backwards {} times (first at frame {}: {} -> {} ps)",
                    back.len(),
                    i + 1,
                    fmt_g(t[i]),
                    fmt_g(t[i + 1])
                ),
            ));
        }
        if let Some(&i) = same.first() {
            out.push(finding("time", Level::Warn, &fr.name, format!(
                "{} frames repeat the previous frame's time (first at frame {}, {} ps)", same.len(), i + 1, fmt_g(t[i]))));
        }
        let pos: Vec<f64> = d.iter().copied().filter(|x| *x > 0.0).collect();
        if !pos.is_empty() {
            let step = median(pos.clone());
            let gaps = pos
                .iter()
                .filter(|x| (*x - step).abs() > 1e-3 * step.max(1e-9))
                .count();
            if gaps > 0 {
                let largest = pos.iter().copied().fold(0.0, f64::max);
                out.push(finding("time", Level::Info, &fr.name, format!(
                    "uneven spacing: {gaps} of {} steps differ from the median {} ps (largest {} ps)", pos.len(), fmt_g(step), fmt_g(largest))));
            }
        }
    }

    let full = trajs.iter().find(|f| f.name == "full.xtc");
    let mini = trajs.iter().find(|f| f.name == "minimal.xtc");
    if let (Some(f), Some(m)) = (full, mini) {
        if f.times.len() != m.times.len() {
            out.push(finding(
                "time",
                Level::Fail,
                "minimal.xtc",
                format!("{} frames; full.xtc has {}", m.times.len(), f.times.len()),
            ));
        } else if f
            .times
            .iter()
            .zip(&m.times)
            .any(|(a, b)| (a - b).abs() > 1e-3)
        {
            out.push(finding(
                "time",
                Level::Fail,
                "minimal.xtc",
                "frame times differ from full.xtc's".into(),
            ));
        }
    }

    if let (Some(f), Some(src)) = (full, source_times) {
        if src.len() != f.times.len() {
            out.push(finding(
                "time",
                Level::Warn,
                "full.xtc",
                format!(
                    "{} frames; the source {source_name} has {}",
                    f.times.len(),
                    src.len()
                ),
            ));
        } else if !src.is_empty() {
            let s: Vec<f64> = src.iter().map(|x| x - src[0]).collect();
            let g: Vec<f64> = f.times.iter().map(|x| x - f.times[0]).collect();
            let spacing = if s.len() > 1 {
                median(s.windows(2).map(|w| w[1] - w[0]).collect())
            } else {
                1.0
            };
            let tol = 1e-3 * spacing.max(1e-3);
            let bad: Vec<usize> = (0..s.len())
                .filter(|&i| (s[i] - g[i]).abs() > tol)
                .collect();
            if let Some(&i) = bad.first() {
                out.push(finding("time", Level::Fail, "full.xtc", format!(
                    "{} frame times differ from the source's (first at frame {i}: source {} ps from its start, full.xtc {} ps)",
                    bad.len(), fmt_g(s[i]), fmt_g(g[i]))));
            }
        }
    }
    if out.is_empty() {
        out.push(finding(
            "time",
            Level::Info,
            "",
            "frame times in order".into(),
        ));
    }
    out
}

// --- check 3: atoms ----------------------------------------------------------------
/// Equal, or equal as far as the narrower format can hold
fn same_name(a: &str, b: &str, width: usize) -> bool {
    if a == b {
        return true;
    }
    if a.chars().count() > width || b.chars().count() > width {
        return a.chars().take(width).eq(b.chars().take(width));
    }
    false
}

/// 1-based index of each residue's first atom: a new residue starts where
/// the residue number or name changes
pub fn residue_starts(atoms: &[Atom]) -> Vec<usize> {
    let mut starts = vec![];
    let mut prev: Option<(i64, &str)> = None;
    for (i, a) in atoms.iter().enumerate() {
        let key = (a.resid, a.resname.as_str());
        if prev != Some(key) {
            starts.push(i + 1);
            prev = Some(key);
        }
    }
    starts
}

fn is_element(s: &str) -> bool {
    ELEMENTS
        .split_whitespace()
        .any(|e| e.eq_ignore_ascii_case(s))
}

pub fn check_atom_set(
    label: &str,
    structs: &[Structure],
    trajs: &[&Frames],
) -> Vec<Finding> {
    let mut out = vec![];
    let mut counts: Vec<(String, usize)> = structs
        .iter()
        .map(|s| (s.name.clone(), s.atoms.len()))
        .collect();
    counts.extend(trajs.iter().map(|f| (f.name.clone(), f.natoms)));
    if counts.iter().map(|c| c.1).collect::<HashSet<_>>().len() > 1 {
        let list: Vec<String> =
            counts.iter().map(|(n, c)| format!("{n} {c}")).collect();
        out.push(finding(
            "atoms",
            Level::Fail,
            label,
            format!("atom counts differ: {}", list.join(", ")),
        ));
        return out;
    }

    // Narrowest format in the whole set: the .mmcif takes its names from the
    // .pdb, so a long .psf name reaches it already cut to 4.
    let nw = structs
        .iter()
        .filter_map(|s| s.name_width)
        .chain([8])
        .min()
        .unwrap_or(8);
    let rw = structs
        .iter()
        .filter_map(|s| s.resname_width)
        .chain([8])
        .min()
        .unwrap_or(8);
    let wrap = structs.iter().filter_map(|s| s.resid_wrap).min();

    let reference = structs.first();
    for s in structs {
        let mut bad_el: Vec<&str> = s
            .atoms
            .iter()
            .map(|a| a.element.as_str())
            .filter(|e| !e.is_empty() && !is_element(e))
            .collect();
        bad_el.sort();
        bad_el.dedup();
        if !bad_el.is_empty() {
            out.push(finding(
                "atoms",
                Level::Fail,
                &s.name,
                format!("not element symbols: {}", bad_el.join(", ")),
            ));
        }
        let Some(r) = reference else { continue };
        if std::ptr::eq(r, s) {
            continue;
        }
        let num = |x: i64| wrap.map_or(x, |w| x.rem_euclid(w));
        let mut renumbered = None;
        let mut differs = None;
        for (i, (a, b)) in r.atoms.iter().zip(&s.atoms).enumerate() {
            let what = if !same_name(&a.name, &b.name, nw) {
                Some(format!("atom name {:?} vs {:?}", a.name, b.name))
            } else if !same_name(&a.resname, &b.resname, rw) {
                Some(format!("residue name {:?} vs {:?}", a.resname, b.resname))
            } else if !a.element.is_empty()
                && !b.element.is_empty()
                && !a.element.eq_ignore_ascii_case(&b.element)
            {
                Some(format!("element {:?} vs {:?}", a.element, b.element))
            } else {
                if num(a.resid) != num(b.resid) && renumbered.is_none() {
                    renumbered = Some((i, a.resid, b.resid));
                }
                None
            };
            if let Some(w) = what {
                differs = Some(format!("atom {} differs from {}: {w}", i + 1, r.name));
                break;
            }
        }
        if let Some(d) = differs {
            out.push(finding("atoms", Level::Fail, &s.name, d));
            continue;
        }
        let (rb, sb) = (residue_starts(&r.atoms), residue_starts(&s.atoms));
        if rb != sb {
            let i = (0..rb.len().max(sb.len()))
                .find(|&k| rb.get(k) != sb.get(k))
                .unwrap_or(0);
            out.push(finding("atoms", Level::Fail, &s.name, format!(
                "residues split differently from {} (residue {}: starts at atom {:?} vs {:?})", r.name, i + 1, rb.get(i), sb.get(i))));
        } else if let Some((i, x, y)) = renumbered {
            out.push(finding("atoms", Level::Info, &s.name, format!(
                "residues numbered differently from {} (from atom {}: residue {x} vs {y})", r.name, i + 1)));
        }
    }

    if let Some(first) = trajs
        .iter()
        .find_map(|f| f.first_xyz.as_ref().map(|x| (f, x)))
    {
        let (tr, fx) = first;
        for s in structs {
            let Some(xyz) = &s.xyz else { continue };
            if xyz.len() != fx.len() {
                continue;
            }
            let dev: Vec<f64> = xyz
                .iter()
                .zip(fx.iter())
                .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f64::max))
                .collect();
            let (worst, max) = dev.iter().enumerate().fold((0, 0.0), |acc, (i, &d)| {
                if d > acc.1 { (i, d) } else { acc }
            });
            if max > COORD_TOL_A {
                let n = dev.iter().filter(|&&d| d > COORD_TOL_A).count();
                out.push(finding("atoms", Level::Fail, &s.name, format!(
                    "coordinates differ from {} frame 0 by up to {max:.3} A (atom {}); {n} atoms beyond {COORD_TOL_A} A", tr.name, worst + 1)));
            }
        }
    }
    out
}

pub fn check_atoms(files: &HashMap<String, PathBuf>, trajs: &[Frames]) -> Vec<Finding> {
    let mut out = vec![];
    for label in ["full", "minimal"] {
        let mut structs = vec![];
        // .psf first: it is the topology the others are checked against.
        let readers: [(&str, fn(&Path) -> Result<Structure>); 3] =
            [("psf", read_psf), ("pdb", read_pdb), ("mmcif", read_mmcif)];
        for (ext, reader) in readers {
            let name = format!("{label}.{ext}");
            if let Some(p) = files.get(&name) {
                match reader(p) {
                    Ok(s) => structs.push(s),
                    Err(e) => out.push(finding(
                        "atoms",
                        Level::Fail,
                        &name,
                        format!("cannot be read: {e}"),
                    )),
                }
            }
        }
        let mut names = vec![format!("{label}.xtc")];
        if label == "minimal" {
            names.push("sampled.xtc".into());
        }
        let tr: Vec<&Frames> = names
            .iter()
            .filter_map(|n| trajs.iter().find(|f| &f.name == n))
            .collect();
        if structs.len() + tr.len() >= 2 {
            out.extend(check_atom_set(label, &structs, &tr));
        }
    }
    if out.is_empty() {
        out.push(finding("atoms", Level::Info, "", "files agree".into()));
    }
    out
}

// --------------------------------------------------
/// {file name: path} for the names the checks know; the first dir wins
pub fn find_files(dirs: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut names: Vec<String> = ["full", "minimal"]
        .iter()
        .flat_map(|s| ["xtc", "pdb", "psf", "mmcif"].map(|e| format!("{s}.{e}")))
        .collect();
    names.push("sampled.xtc".into());
    let mut found = HashMap::new();
    for d in dirs {
        for n in &names {
            let p = d.join(n);
            if !found.contains_key(n) && p.is_file() {
                found.insert(n.clone(), p);
            }
        }
    }
    found
}

/// All three checks over the files found in `dirs`
pub fn run_checks(
    dirs: &[PathBuf],
    box_present: Option<bool>,
    source_trajectory: Option<&Path>,
) -> Result<Vec<Finding>> {
    let files = find_files(dirs);
    let (mut trajs, mut out) = (vec![], vec![]);
    for n in ["full.xtc", "minimal.xtc", "sampled.xtc"] {
        if let Some(p) = files.get(n) {
            match read_xtc(p) {
                Ok(f) => trajs.push(f),
                Err(e) => out.push(finding(
                    "read",
                    Level::Fail,
                    n,
                    format!("cannot be read: {e}"),
                )),
            }
        }
    }
    out.extend(check_box(&trajs, &files, box_present)?);
    let mut source_name = String::new();
    let mut source = None;
    if let Some(src) = source_trajectory {
        source_name = base_name(src);
        match read_source_times(src) {
            Ok(t) => source = t,
            Err(e) => out.push(finding(
                "time",
                Level::Warn,
                &source_name,
                format!("source times not read: {e}"),
            )),
        }
    }
    out.extend(check_time(&trajs, source.as_deref(), &source_name));
    out.extend(check_atoms(&files, &trajs));
    Ok(out)
}

// --------------------------------------------------
/// Every rule both ways, with files built here: XTCs written through molly
/// (3 atoms, uncompressed, and 12, compressed), PDB/PSF/mmCIF as text in the
/// shapes our pipeline writes. The cases that shaped the rules on real
/// outputs (2026-11-09) are here too: cpptraj's 4-letter residue names from
/// column 17 (TIP3, MDR00001511), .psf numbering restarting per segment
/// (info), a box rotated by the RMS fit, released sampled.xtc with every
/// frame at 0 ps, and MDR00105222's box.
#[cfg(test)]
mod tests {
    use super::*;

    type A = (&'static str, &'static str, i64, &'static str);
    const ATOMS: [A; 6] = [
        ("N", "LYS", 15, "N"),
        ("CA", "LYS", 15, "C"),
        ("C", "LYS", 15, "C"),
        ("N", "GLY", 16, "N"),
        ("CA", "GLY", 16, "C"),
        ("OH2", "TIP3", 17, "O"),
    ];
    const XYZ: [Xyz; 6] = [
        [1.0, 2.0, 3.0],
        [2.4, 2.1, 3.0],
        [3.0, 3.4, 3.1],
        [4.2, 3.6, 3.2],
        [5.0, 4.8, 3.4],
        [9.0, 9.0, 9.0],
    ];
    const CUBE: Mat3 = [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 5.0]];
    const ZERO_BOX: Mat3 = [[0.0; 3]; 3];
    const INF_BOX: Mat3 = [
        [f64::INFINITY, f64::INFINITY, 0.0],
        [2.06, f64::NEG_INFINITY, f64::NEG_INFINITY],
        [0.0; 3],
    ];

    fn write_xtc(path: &Path, xyz: &[Xyz], boxes: &[Mat3], times: &[f64]) -> PathBuf {
        let mut w = molly::XTCWriter::create(path).unwrap();
        for (i, (b, t)) in boxes.iter().zip(times).enumerate() {
            w.write_frame(&molly::Frame {
                step: i as u32,
                time: *t as f32,
                boxvec: b
                    .iter()
                    .flatten()
                    .map(|v| *v as f32)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                precision: 1000.0,
                positions: xyz.iter().flatten().map(|v| (*v / 10.0) as f32).collect(),
            })
            .unwrap();
        }
        path.to_path_buf()
    }

    fn xtc(path: &Path, n: usize, b: Mat3) -> PathBuf {
        let times: Vec<f64> = (0..3).map(|i| 10.0 * i as f64).collect();
        write_xtc(path, &XYZ[..n], &[b; 3], &times)
    }

    fn pdb_line(i: usize, a: &A, x: &Xyz) -> String {
        // cpptraj: a 4-letter atom name from column 13, else 14; a 4-letter
        // residue name from column 17.
        let name = if a.0.len() == 4 {
            a.0.to_string()
        } else {
            format!(" {:<3}", a.0)
        };
        let res = if a.1.len() == 4 {
            format!("{:>4}", a.1)
        } else {
            format!(" {:<3}", a.1)
        };
        format!(
            "ATOM  {i:5} {name}{res} A{:4}    {:8.3}{:8.3}{:8.3}  1.00  0.00          {:>2}\n",
            a.2, x[0], x[1], x[2], a.3
        )
    }

    fn write_pdb(
        path: &Path,
        atoms: &[A],
        xyz: &[Xyz],
        cryst1: Option<[f64; 6]>,
        models: usize,
    ) -> PathBuf {
        let mut s = String::new();
        if let Some(c) = cryst1 {
            s += &format!(
                "CRYST1{:9.3}{:9.3}{:9.3}{:7.2}{:7.2}{:7.2} P 1           1\n",
                c[0], c[1], c[2], c[3], c[4], c[5]
            );
        }
        for m in 0..models {
            if models > 1 {
                s += &format!("MODEL     {:4}\n", m + 1);
            }
            for (i, (a, x)) in atoms.iter().zip(xyz).enumerate() {
                s += &pdb_line(i + 1, a, &x.map(|v| v + m as f64));
            }
            if models > 1 {
                s += "ENDMDL\n";
            }
        }
        fs::write(path, s + "END\n").unwrap();
        path.to_path_buf()
    }

    fn write_psf(path: &Path, atoms: &[A]) -> PathBuf {
        let mut s = format!(
            "PSF EXT\n\n       1 !NTITLE\n REMARKS test\n\n{:10} !NATOM\n",
            atoms.len()
        );
        for (i, a) in atoms.iter().enumerate() {
            s += &format!(
                "{:10} PROA     {:<8} {:<8} {:<8} {:<6}  0.000000  12.0110  0\n",
                i + 1,
                a.2,
                a.1,
                a.0,
                a.0
            );
        }
        fs::write(path, s + "\n         0 !NBOND: bonds\n").unwrap();
        path.to_path_buf()
    }

    fn write_mmcif(path: &Path, atoms: &[A], xyz: &[Xyz]) -> PathBuf {
        let mut s = String::from(
            "data_test\n#\n_struct.title 'a title with spaces'\n#\nloop_\n_atom_site.group_PDB\n_atom_site.id\n\
             _atom_site.type_symbol\n_atom_site.auth_atom_id\n_atom_site.auth_comp_id\n\
             _atom_site.auth_asym_id\n_atom_site.auth_seq_id\n_atom_site.Cartn_x\n\
             _atom_site.Cartn_y\n_atom_site.Cartn_z\n_atom_site.pdbx_PDB_model_num\n",
        );
        for (i, (a, x)) in atoms.iter().zip(xyz).enumerate() {
            let name = if a.0.contains('\'') {
                format!("\"{}\"", a.0)
            } else {
                a.0.to_string()
            };
            s += &format!(
                "ATOM {} {} {name} {} A {} {:.3} {:.3} {:.3} 1\n",
                i + 1,
                a.3,
                a.1,
                a.2,
                x[0],
                x[1],
                x[2]
            );
        }
        fs::write(path, s + "#\n").unwrap();
        path.to_path_buf()
    }

    fn full_set(d: &Path, b: Mat3, cryst1: Option<[f64; 6]>) -> PathBuf {
        fs::create_dir_all(d).unwrap();
        write_psf(&d.join("full.psf"), &ATOMS);
        write_pdb(&d.join("full.pdb"), &ATOMS, &XYZ, cryst1, 1);
        write_mmcif(&d.join("full.mmcif"), &ATOMS, &XYZ);
        xtc(&d.join("full.xtc"), 6, b);
        write_psf(&d.join("minimal.psf"), &ATOMS[..5]);
        write_pdb(&d.join("minimal.pdb"), &ATOMS[..5], &XYZ[..5], None, 1);
        xtc(&d.join("minimal.xtc"), 5, b);
        d.to_path_buf()
    }

    const C50: Option<[f64; 6]> = Some([50.0, 50.0, 50.0, 90.0, 90.0, 90.0]);

    fn run(d: &Path, bp: Option<bool>) -> Vec<Finding> {
        run_checks(&[d.to_path_buf()], bp, None).unwrap()
    }

    fn levels(f: &[Finding], check: Option<&str>) -> Vec<(Level, String)> {
        f.iter()
            .filter(|x| x.level != Level::Info && check.is_none_or(|c| x.check == c))
            .map(|x| (x.level, x.file.clone()))
            .collect()
    }

    fn fail(file: &str) -> (Level, String) {
        (Level::Fail, file.into())
    }

    // --- box geometry ------------------------------------------------------
    #[test]
    fn box_kinds() {
        let rows = |a: Xyz, b: Xyz, c: Xyz| [a, b, c];
        for (b, kind) in [
            (ZERO_BOX, "zero"),
            ([[1e-8; 3]; 3], "zero"),
            (CUBE, "cell"),
            (INF_BOX, "non-finite"),
            ([[f64::NAN; 3]; 3], "non-finite"),
            (rows([-7083.2, 0.0, 0.0], [0.0; 3], [0.0; 3]), "bad"), // 105222 rep_10
            (
                rows([1992603.5, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 5.0]),
                "bad",
            ),
            (
                rows([5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 0.01]),
                "bad",
            ),
            (
                rows([5.0, 0.0, 0.0], [5.0, 0.001, 0.0], [0.0, 0.0, 5.0]),
                "bad",
            ), // flat
            (
                rows([5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [2.5, 2.5, 3.54]),
                "cell",
            ), // triclinic
        ] {
            assert_eq!(box_kind(&b), kind, "{b:?}");
        }
    }

    #[test]
    fn a_rotated_cell_is_the_same_cell() {
        let (s, c) = 37f64.to_radians().sin_cos();
        let b: Mat3 = [[5.0, 0.0, 0.0], [0.0, 6.0, 0.0], [1.0, 1.0, 7.0]];
        let rot = b.map(|r| [c * r[0] - s * r[1], s * r[0] + c * r[1], r[2]]);
        assert_eq!(box_kind(&rot), "cell");
        assert!(same_cell(&b, &rot));
        assert!(!same_cell(&b, &b.map(|r| r.map(|v| v * 1.01))));
    }

    #[test]
    fn cryst1_round_trip() {
        let (l, a) =
            lengths_angles(&box_from_cryst1(&[50.0, 60.0, 70.0, 80.0, 85.0, 95.0]));
        for (x, y) in l
            .iter()
            .zip([5.0, 6.0, 7.0])
            .chain(a.iter().zip([80.0, 85.0, 95.0]))
        {
            assert!((x - y).abs() < 1e-9, "{l:?} {a:?}");
        }
    }

    // --- readers -------------------------------------------------------------
    #[test]
    fn readers_agree_on_our_shapes() {
        let d = tempfile::tempdir().unwrap();
        let p = read_pdb(&write_pdb(&d.path().join("x.pdb"), &ATOMS, &XYZ, None, 1))
            .unwrap();
        let s = read_psf(&write_psf(&d.path().join("x.psf"), &ATOMS)).unwrap();
        let c =
            read_mmcif(&write_mmcif(&d.path().join("x.mmcif"), &ATOMS, &XYZ)).unwrap();
        assert_eq!(p.atoms.last().unwrap().resname, "TIP3"); // column 17
        let key = |s: &Structure| {
            s.atoms
                .iter()
                .map(|a| (a.name.clone(), a.resname.clone(), a.resid))
                .collect::<Vec<_>>()
        };
        assert_eq!(key(&p), key(&s));
        assert_eq!(key(&p), key(&c));
        assert_eq!(p.xyz, c.xyz);
        assert_eq!(c.atoms[0].element, "N");
    }

    #[test]
    fn pdb_reads_the_first_model_only() {
        let d = tempfile::tempdir().unwrap();
        let p = read_pdb(&write_pdb(&d.path().join("m.pdb"), &ATOMS, &XYZ, None, 3))
            .unwrap();
        assert_eq!(p.atoms.len(), 6);
    }

    #[test]
    fn mmcif_quoted_names_and_second_model() {
        let d = tempfile::tempdir().unwrap();
        let atoms: [A; 2] = [("O5'", "DA", 1, "O"), ("C5'", "DA", 1, "C")];
        let p = write_mmcif(&d.path().join("q.mmcif"), &atoms, &XYZ[..2]);
        let mut text = fs::read_to_string(&p).unwrap();
        text = text.replace("#\n", "") + "ATOM 3 C X DA A 1 0 0 0 2\n";
        fs::write(&p, text).unwrap();
        let c = read_mmcif(&p).unwrap();
        assert_eq!(
            c.atoms.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["O5'", "C5'"]
        );
    }

    #[test]
    fn cif_tokens_handle_quotes() {
        assert_eq!(
            cif_tokens(r#"ATOM 1 "O5'" 'a b' x"#),
            ["ATOM", "1", "O5'", "a b", "x"]
        );
    }

    #[test]
    fn a_large_compressed_xtc_reads_back() {
        let d = tempfile::tempdir().unwrap();
        let xyz: Vec<Xyz> = (0..12).map(|i| [i as f64, 2.0 * i as f64, 3.0]).collect();
        let p = write_xtc(
            &d.path().join("big.xtc"),
            &xyz,
            &[CUBE; 4],
            &[0.0, 5.0, 10.0, 15.0],
        );
        let f = read_xtc(&p).unwrap();
        assert_eq!((f.natoms, f.times.len()), (12, 4));
        let first = f.first_xyz.unwrap();
        assert!(
            first
                .iter()
                .zip(&xyz)
                .all(|(a, b)| (0..3).all(|k| (a[k] - b[k]).abs() < 0.011))
        );
    }

    #[test]
    fn a_damaged_xtc_is_an_error_not_a_panic() {
        let d = tempfile::tempdir().unwrap();
        let p = xtc(&d.path().join("x.xtc"), 3, CUBE);
        let mut bytes = fs::read(&p).unwrap();
        bytes[55] ^= 0x7f; // the repeated atom count (bytes 52-55)
        fs::write(&p, bytes).unwrap();
        assert!(read_xtc(&p).is_err());
    }

    // --- check 1: box ----------------------------------------------------------
    #[test]
    fn a_consistent_box_passes() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            levels(
                &run(&full_set(&d.path().join("s"), CUBE, C50), Some(true)),
                None
            ),
            vec![]
        );
    }

    #[test]
    fn no_box_passes_with_no_box() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            levels(
                &run(&full_set(&d.path().join("s"), ZERO_BOX, None), Some(false)),
                None
            ),
            vec![]
        );
    }

    #[test]
    fn the_105222_box_fails() {
        let d = tempfile::tempdir().unwrap();
        let f = run(&full_set(&d.path().join("s"), INF_BOX, None), None);
        let box_ = levels(&f, Some("box"));
        assert!(
            box_.contains(&fail("full.xtc")) && box_.contains(&fail("minimal.xtc")),
            "{f:?}"
        );
        let m = &f.iter().find(|x| x.file == "full.xtc").unwrap().message;
        assert!(m.starts_with("3 of 3 frames have a non-finite box (first at frame 0: [inf, inf, 0, 2.06"), "{m}");
    }

    #[test]
    fn some_frames_with_a_box_and_some_without_fails() {
        let d = tempfile::tempdir().unwrap();
        let p = write_xtc(
            &d.path().join("full.xtc"),
            &XYZ[..3],
            &[CUBE, ZERO_BOX, CUBE],
            &[0.0, 1.0, 2.0],
        );
        let f = check_box(&[read_xtc(&p).unwrap()], &HashMap::new(), None).unwrap();
        assert_eq!(levels(&f, None), vec![fail("full.xtc")]);
        assert!(f[0].message.contains("1 frames have no box and 2 have one"));
    }

    #[test]
    fn the_box_must_match_the_pipelines_decision() {
        for (bp, b) in [(true, ZERO_BOX), (false, CUBE)] {
            let d = tempfile::tempdir().unwrap();
            let fr = read_xtc(&xtc(&d.path().join("full.xtc"), 3, b)).unwrap();
            let f = check_box(&[fr], &HashMap::new(), Some(bp)).unwrap();
            assert_eq!(levels(&f, None), vec![fail("full.xtc")], "{bp}");
        }
    }

    #[test]
    fn sampled_may_have_no_box_when_the_run_has_one() {
        let d = tempfile::tempdir().unwrap();
        let fr = read_xtc(&xtc(&d.path().join("sampled.xtc"), 3, ZERO_BOX)).unwrap();
        assert_eq!(
            levels(
                &check_box(&[fr], &HashMap::new(), Some(true)).unwrap(),
                None
            ),
            vec![]
        );
    }

    #[test]
    fn cryst1_must_match_the_first_frame() {
        let d = tempfile::tempdir().unwrap();
        let f = run(
            &full_set(
                &d.path().join("s"),
                CUBE,
                Some([52.0, 50.0, 50.0, 90.0, 90.0, 90.0]),
            ),
            None,
        );
        assert!(levels(&f, Some("box")).contains(&fail("full.pdb")), "{f:?}");
    }

    #[test]
    fn missing_cryst1_with_a_box_warns() {
        let d = tempfile::tempdir().unwrap();
        let f = run(&full_set(&d.path().join("s"), CUBE, None), None);
        assert_eq!(
            levels(&f, Some("box")),
            vec![(Level::Warn, "full.pdb".into())]
        );
    }

    #[test]
    fn a_cryst1_without_a_box_fails_but_the_no_box_cryst1_passes() {
        let d = tempfile::tempdir().unwrap();
        let f = run(&full_set(&d.path().join("a"), ZERO_BOX, C50), None);
        assert!(levels(&f, Some("box")).contains(&fail("full.pdb")), "{f:?}");
        let f = run(
            &full_set(
                &d.path().join("b"),
                ZERO_BOX,
                Some([1.0, 1.0, 1.0, 90.0, 90.0, 90.0]),
            ),
            None,
        );
        assert_eq!(levels(&f, Some("box")), vec![]);
    }

    // --- check 2: time -----------------------------------------------------------
    fn frames(d: &Path, name: &str, times: &[f64]) -> Frames {
        read_xtc(&write_xtc(
            &d.join(name),
            &XYZ[..3],
            &vec![CUBE; times.len()],
            times,
        ))
        .unwrap()
    }

    #[test]
    fn times_in_order_pass() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            levels(
                &check_time(
                    &[frames(d.path(), "full.xtc", &[0.0, 10.0, 20.0])],
                    None,
                    ""
                ),
                None
            ),
            vec![]
        );
    }

    #[test]
    fn time_going_backwards_fails() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[frames(d.path(), "full.xtc", &[0.0, 10.0, 5.0, 20.0])],
            None,
            "",
        );
        assert_eq!(levels(&f, None), vec![fail("full.xtc")]);
        assert!(f[0].message.contains("10 -> 5 ps"), "{}", f[0].message);
    }

    #[test]
    fn a_repeated_frame_time_warns() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[frames(d.path(), "full.xtc", &[0.0, 10.0, 10.0, 20.0])],
            None,
            "",
        );
        assert_eq!(levels(&f, None), vec![(Level::Warn, "full.xtc".into())]);
        assert!(f[0].message.contains("repeat"));
    }

    #[test]
    fn no_time_axis_warns_once() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(&[frames(d.path(), "sampled.xtc", &[0.0; 5])], None, "");
        assert_eq!(levels(&f, None), vec![(Level::Warn, "sampled.xtc".into())]);
        assert_eq!(f[0].message, "no time axis: all 5 frames are at 0 ps");
    }

    #[test]
    fn non_finite_times_fail() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[frames(d.path(), "full.xtc", &[0.0, f64::NAN, 2.0])],
            None,
            "",
        );
        assert_eq!(levels(&f, None), vec![fail("full.xtc")]);
    }

    #[test]
    fn skipped_time_is_only_reported() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[frames(
                d.path(),
                "full.xtc",
                &[0.0, 10.0, 20.0, 100.0, 110.0],
            )],
            None,
            "",
        );
        assert_eq!(levels(&f, None), vec![]);
        assert!(f.iter().any(|x| x.level == Level::Info && x.message.contains("uneven spacing")));
    }

    #[test]
    fn full_and_minimal_must_have_the_same_frames() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[
                frames(d.path(), "full.xtc", &[0.0, 10.0, 20.0]),
                frames(d.path(), "minimal.xtc", &[0.0, 10.0]),
            ],
            None,
            "",
        );
        assert_eq!(levels(&f, None), vec![fail("minimal.xtc")]);
        let f = check_time(
            &[
                frames(d.path(), "full.xtc", &[0.0, 10.0, 20.0]),
                frames(d.path(), "minimal.xtc", &[0.0, 10.0, 30.0]),
            ],
            None,
            "",
        );
        assert_eq!(levels(&f, None), vec![fail("minimal.xtc")]);
    }

    #[test]
    fn source_times_match_from_any_start() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[frames(d.path(), "full.xtc", &[0.0, 10.0, 20.0, 100.0])],
            Some(&[1000.0, 1010.0, 1020.0, 1100.0]),
            "s.xtc",
        );
        assert_eq!(levels(&f, None), vec![]);
    }

    #[test]
    fn a_grid_stamped_over_skipped_source_time_fails() {
        let d = tempfile::tempdir().unwrap();
        let f = check_time(
            &[frames(d.path(), "full.xtc", &[0.0, 10.0, 20.0, 30.0])],
            Some(&[0.0, 10.0, 20.0, 100.0]),
            "s.xtc",
        );
        assert_eq!(levels(&f, None), vec![fail("full.xtc")]);
        assert!(f[0].message.contains("source 100 ps"), "{}", f[0].message);
    }

    #[test]
    fn a_different_frame_count_from_the_source_warns() {
        let d = tempfile::tempdir().unwrap();
        let src: Vec<f64> = (0..130).map(f64::from).collect();
        let f = check_time(
            &[frames(d.path(), "full.xtc", &[0.0, 10.0, 20.0])],
            Some(&src),
            "s.xtc",
        );
        assert_eq!(levels(&f, None), vec![(Level::Warn, "full.xtc".into())]);
    }

    #[test]
    fn source_times_come_from_xtc_only() {
        let d = tempfile::tempdir().unwrap();
        let p = write_xtc(
            &d.path().join("src.xtc"),
            &XYZ[..3],
            &[CUBE; 3],
            &[5.0, 15.0, 25.0],
        );
        assert_eq!(read_source_times(&p).unwrap(), Some(vec![5.0, 15.0, 25.0]));
        assert_eq!(read_source_times(&d.path().join("src.dcd")).unwrap(), None);
    }

    // --- check 3: atoms ------------------------------------------------------------
    #[test]
    fn files_that_agree_pass() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            levels(
                &run(&full_set(&d.path().join("s"), CUBE, C50), None),
                Some("atoms")
            ),
            vec![]
        );
    }

    #[test]
    fn atom_counts_must_agree() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        write_pdb(&s.join("full.pdb"), &ATOMS[..5], &XYZ[..5], C50, 1);
        assert!(levels(&run(&s, None), Some("atoms")).contains(&fail("full")));
    }

    #[test]
    fn one_wrong_atom_fails() {
        for (i, field, value, expect) in [
            (1, 0, "CB", "atom name"),
            (3, 1, "ALA", "residue name"),
            (0, 3, "XP", "not element symbols"),
        ] {
            let d = tempfile::tempdir().unwrap();
            let s = full_set(&d.path().join("s"), CUBE, C50);
            let mut atoms = ATOMS;
            match field {
                0 => atoms[i].0 = value,
                1 => atoms[i].1 = value,
                _ => atoms[i].3 = value,
            }
            write_mmcif(&s.join("full.mmcif"), &atoms, &XYZ);
            let f: Vec<Finding> = run(&s, None)
                .into_iter()
                .filter(|x| x.level == Level::Fail)
                .collect();
            assert!(
                !f.is_empty()
                    && f[0].file == "full.mmcif"
                    && f[0].message.contains(expect),
                "{expect}: {f:?}"
            );
        }
    }

    #[test]
    fn long_names_compare_as_far_as_the_narrow_format_holds() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        let mut atoms = ATOMS;
        atoms[5].1 = "TIP3XX";
        write_psf(&s.join("full.psf"), &atoms);
        assert_eq!(levels(&run(&s, None), Some("atoms")), vec![]);
    }

    #[test]
    fn psf_numbering_per_segment_is_info() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        let mut atoms = ATOMS;
        atoms[5].2 = 1;
        write_psf(&s.join("full.psf"), &atoms);
        let f = run(&s, None);
        assert_eq!(levels(&f, Some("atoms")), vec![]);
        assert!(
            f.iter().any(|x| x.level == Level::Info
                && x.message.contains("numbered differently")),
            "{f:?}"
        );
    }

    #[test]
    fn residues_split_differently_fails() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        let mut atoms: Vec<A> = ATOMS.to_vec();
        atoms[3] = ("N", "LYS", 15, "N"); // GLY's N put in LYS
        atoms[4] = ("CA", "LYS", 16, "C");
        let atoms: Vec<A> = atoms
            .into_iter()
            .map(|a| (a.0, if a.1 == "TIP3" { "TIP3" } else { "LYS" }, a.2, a.3))
            .collect();
        write_pdb(&s.join("full.pdb"), &atoms, &XYZ, C50, 1);
        let f: Vec<Finding> = run(&s, None)
            .into_iter()
            .filter(|x| x.level == Level::Fail && x.check == "atoms")
            .collect();
        assert!(!f.is_empty() && f[0].file == "full.pdb", "{f:?}");
    }

    #[test]
    fn structure_coordinates_must_equal_frame_0() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        let mut moved = XYZ;
        moved[2][0] += 0.5;
        write_pdb(&s.join("full.pdb"), &ATOMS, &moved, C50, 1);
        let f: Vec<Finding> = run(&s, None)
            .into_iter()
            .filter(|x| x.level == Level::Fail)
            .collect();
        assert!(
            f[0].file == "full.pdb" && f[0].message.contains("atom 3"),
            "{f:?}"
        );
    }

    #[test]
    fn xtc_precision_is_not_a_difference() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        let nudged = XYZ.map(|x| x.map(|v| v + 0.004));
        write_pdb(&s.join("full.pdb"), &ATOMS, &nudged, C50, 1);
        assert_eq!(levels(&run(&s, None), Some("atoms")), vec![]);
    }

    #[test]
    fn gro_files_are_ignored() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        fs::write(s.join("full.gro"), "garbage\n 1\n").unwrap();
        assert!(!find_files(std::slice::from_ref(&s)).contains_key("full.gro"));
        assert_eq!(levels(&run(&s, None), None), vec![]);
    }

    #[test]
    fn files_split_across_dirs_first_dir_wins() {
        let d = tempfile::tempdir().unwrap();
        let (orig, new) = (d.path().join("orig"), d.path().join("new"));
        fs::create_dir_all(&orig).unwrap();
        fs::create_dir_all(&new).unwrap();
        write_pdb(&orig.join("minimal.pdb"), &ATOMS[..5], &XYZ[..5], None, 1);
        xtc(&orig.join("sampled.xtc"), 5, CUBE);
        write_psf(&new.join("minimal.psf"), &ATOMS[..5]);
        write_mmcif(&new.join("minimal.mmcif"), &ATOMS[..5], &XYZ[..5]);
        write_pdb(&new.join("minimal.pdb"), &ATOMS[..4], &XYZ[..4], None, 1);
        assert!(
            find_files(&[new.clone(), orig.clone()])["minimal.pdb"].starts_with(&new)
        );
        let f = run_checks(&[new.clone(), orig.clone()], None, None).unwrap();
        assert!(
            levels(&f, Some("atoms")).contains(&fail("minimal")),
            "{f:?}"
        );
        assert_eq!(
            levels(
                &run_checks(&[orig, new], None, None).unwrap(),
                Some("atoms")
            ),
            vec![]
        );
    }

    #[test]
    fn an_unreadable_file_is_a_failure_not_an_error() {
        let d = tempfile::tempdir().unwrap();
        let s = full_set(&d.path().join("s"), CUBE, C50);
        fs::write(s.join("full.mmcif"), "not a cif").unwrap();
        assert!(levels(&run(&s, None), Some("atoms")).contains(&fail("full.mmcif")));
    }

    #[test]
    fn an_empty_dir_checks_nothing_and_passes() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(levels(&run(d.path(), None), None), vec![]);
    }

    #[test]
    fn findings_serialize_for_json() {
        let f = finding("box", Level::Fail, "full.xtc", "m".into());
        assert_eq!(
            serde_json::to_string(&f).unwrap(),
            r#"{"check":"box","level":"fail","file":"full.xtc","message":"m"}"#
        );
    }

    #[test]
    fn short_numbers() {
        assert_eq!(fmt_g(2.0635762214660645), "2.064");
        assert_eq!(fmt_g(f64::NEG_INFINITY), "-inf");
        assert_eq!(fmt_g(0.0), "0");
        assert_eq!(fmt_g(-6.9e14), "-6.900e14");
        assert_eq!(fmt_g(100.0), "100");
    }

    /// The outputs this was tuned on, when present (the ddd host):
    /// MDR_CHECK_OUTPUTS_DIRS=dir:dir cargo test -p mdr-process -- --ignored real_outputs
    #[test]
    #[ignore = "needs processed folders in MDR_CHECK_OUTPUTS_DIRS"]
    fn real_outputs_pass() {
        let dirs =
            std::env::var("MDR_CHECK_OUTPUTS_DIRS").expect("MDR_CHECK_OUTPUTS_DIRS");
        for d in dirs.split(':') {
            let fails: Vec<Finding> = run(Path::new(d), None)
                .into_iter()
                .filter(|x| x.level == Level::Fail)
                .collect();
            assert!(fails.is_empty(), "{d}: {fails:?}");
        }
    }
}
