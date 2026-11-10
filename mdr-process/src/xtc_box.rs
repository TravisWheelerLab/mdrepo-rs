//! Set the box of every frame to zero in XTC files, alone or inside a tar.
//!
//! Why: cpptraj writes an UNINITIALIZED box when its input has none (a DCD
//! with the unit-cell flag off). Until simulation-processing #44 added `box
//! nobox`, such runs shipped XTCs whose boxes were inf, -inf or 1e10 nm --
//! in every frame of some replicates, none of others. The coordinates are
//! fine. A box-less simulation's box is zero, which is what #44 now writes,
//! so the repair is 36 bytes per frame and nothing else.
//!
//! An XTC frame starts with magic, natoms, step, time and the 3x3 box as
//! big-endian f32 (bytes 16..52 of the frame), then natoms again and the
//! coordinates: 3 x natoms f32 when natoms <= 9, else precision, min/max
//! ints, smallidx, a byte count and that many bytes padded to 4. So frames
//! can be walked by reading headers and seeking, and the box rewritten in
//! place without touching a coordinate.
//!
//! A tar is 512-byte headers each followed by its member's data padded to
//! 512; the header checksum covers the header only. So an XTC member can be
//! patched inside the tar with the tar's size and every header unchanged.
//!
//! Safety: the input is never written. With an output, the patched copy is
//! written beside it as `<out>.partial` and renamed only after it is read
//! back: the same frames, every box zero, and every byte outside the box
//! fields identical to the input. A file holding a real-looking cell in
//! any frame is refused: that simulation has a box, and zeroing it would
//! destroy data.

use crate::output_checks::box_kind;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const XTC_MAGIC: i32 = 1995;
const BOX_OFFSET: u64 = 16;
const BOX_LEN: usize = 36;
const BLOCK: u64 = 512;

// --------------------------------------------------
/// One frame's box: where its 36 bytes start, and their values
#[derive(Debug, Clone)]
pub struct FrameBox {
    pub offset: u64,
    pub raw: [u8; BOX_LEN],
}

impl FrameBox {
    fn values(&self) -> [[f64; 3]; 3] {
        let mut m = [[0.0; 3]; 3];
        for (i, c) in self.raw.as_chunks::<4>().0.iter().enumerate() {
            m[i / 3][i % 3] = f32::from_be_bytes(*c) as f64;
        }
        m
    }

    fn is_zero_bytes(&self) -> bool {
        self.raw.iter().all(|&b| b == 0)
    }
}

/// What was (or would be) done to one XTC: a file, or a tar member
#[derive(Debug, Clone, Serialize)]
pub struct XtcReport {
    /// The file name, or "<tar>:<member>"
    pub name: String,
    pub frames: usize,
    /// Frames whose box bytes are not already all zero
    pub to_zero: usize,
    /// Frames by box kind before: zero, cell, bad, non-finite
    pub kinds: BTreeMap<&'static str, usize>,
}

/// An XTC's frames found in a file: a name and every frame's box
#[derive(Debug, Clone)]
struct Xtc {
    name: String,
    boxes: Vec<FrameBox>,
}

// --------------------------------------------------
fn read_at(f: &mut File, offset: u64, buf: &mut [u8]) -> Result<()> {
    f.seek(SeekFrom::Start(offset))?;
    f.read_exact(buf)
        .with_context(|| format!("short read of {} bytes at {offset}", buf.len()))
}

fn be_i32(b: &[u8]) -> i32 {
    i32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Every frame's box in the XTC occupying bytes [start, end) of `f`
fn walk_xtc(f: &mut File, start: u64, end: u64, name: &str) -> Result<Vec<FrameBox>> {
    let mut out = vec![];
    let mut pos = start;
    while pos < end {
        let frame = out.len();
        let ctx = || format!("{name}: frame {frame} at byte {}", pos - start);
        if end - pos < 56 {
            bail!("{}: truncated frame header", ctx());
        }
        let mut h = [0u8; 56];
        read_at(f, pos, &mut h).with_context(ctx)?;
        let magic = be_i32(&h[0..4]);
        if magic != XTC_MAGIC {
            bail!("{}: magic {magic}, not {XTC_MAGIC}", ctx());
        }
        let natoms = be_i32(&h[4..8]);
        let natoms2 = be_i32(&h[52..56]);
        if natoms < 0 || natoms != natoms2 {
            bail!("{}: atom counts {natoms} and {natoms2}", ctx());
        }
        let mut raw = [0u8; BOX_LEN];
        raw.copy_from_slice(&h[16..52]);
        out.push(FrameBox {
            offset: pos + BOX_OFFSET,
            raw,
        });
        let natoms = natoms as u64;
        let next = if natoms <= 9 {
            pos + 56 + 12 * natoms
        } else {
            // precision, minint[3], maxint[3], smallidx, byte count
            if end - pos < 56 + 36 {
                bail!("{}: truncated coordinate header", ctx());
            }
            let mut c = [0u8; 36];
            read_at(f, pos + 56, &mut c).with_context(ctx)?;
            let nbytes = be_i32(&c[32..36]);
            if nbytes < 0 {
                bail!("{}: negative byte count {nbytes}", ctx());
            }
            pos + 56 + 36 + (nbytes as u64).div_ceil(4) * 4
        };
        if next > end {
            bail!("{}: frame runs {} bytes past the end", ctx(), next - end);
        }
        pos = next;
    }
    Ok(out)
}

// --------------------------------------------------
/// A tar numeric field: octal text, or base-256 when the top bit is set
fn tar_number(field: &[u8]) -> Result<u64> {
    if field[0] & 0x80 != 0 {
        let mut n: u64 = (field[0] & 0x7f) as u64;
        for &b in &field[1..] {
            n = n
                .checked_mul(256)
                .and_then(|n| n.checked_add(b as u64))
                .ok_or_else(|| anyhow!("tar size overflows"))?;
        }
        return Ok(n);
    }
    let text: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|e| anyhow!("tar number {text:?}: {e}"))
}

fn cstr(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap_or(&[])).into_owned()
}

/// "path" and "size" from a pax extended header's records ("LEN key=value\n")
fn pax_records(data: &[u8]) -> (Option<String>, Option<u64>) {
    let (mut path, mut size) = (None, None);
    let mut rest = data;
    while !rest.is_empty() {
        let Some(sp) = rest.iter().position(|&b| b == b' ') else {
            break;
        };
        let Ok(len) = std::str::from_utf8(&rest[..sp])
            .unwrap_or("")
            .parse::<usize>()
        else {
            break;
        };
        if len == 0 || len > rest.len() || len <= sp + 1 {
            break;
        }
        let rec = &rest[sp + 1..len];
        let rec = rec.strip_suffix(b"\n").unwrap_or(rec);
        if let Some(eq) = rec.iter().position(|&b| b == b'=') {
            let (k, v) = (&rec[..eq], &rec[eq + 1..]);
            let v = String::from_utf8_lossy(v).into_owned();
            match k {
                b"path" => path = Some(v),
                b"size" => size = v.parse().ok(),
                _ => {}
            }
        }
        rest = &rest[len..];
    }
    (path, size)
}

/// Every .xtc member's frames in a tar
fn walk_tar(f: &mut File, len: u64, tar_name: &str) -> Result<Vec<Xtc>> {
    let mut out = vec![];
    let mut pos = 0u64;
    let (mut long_name, mut pax_path, mut pax_size) = (None, None, None);
    while pos + BLOCK <= len {
        let mut h = [0u8; BLOCK as usize];
        read_at(f, pos, &mut h)?;
        if h.iter().all(|&b| b == 0) {
            break; // end-of-archive blocks
        }
        let sum: u64 = h
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    b as u64
                }
            })
            .sum();
        if tar_number(&h[148..156])? != sum {
            bail!("{tar_name}: bad tar header checksum at byte {pos}");
        }
        let mut size = tar_number(&h[124..136])?;
        let kind = h[156];
        let data = pos + BLOCK;
        match kind {
            b'L' => {
                let mut name = vec![0u8; size as usize];
                read_at(f, data, &mut name)?;
                long_name = Some(cstr(&name));
            }
            b'x' => {
                let mut rec = vec![0u8; size as usize];
                read_at(f, data, &mut rec)?;
                (pax_path, pax_size) = pax_records(&rec);
            }
            b'g' => {}
            b'S' => bail!("{tar_name}: sparse member at byte {pos}; not handled"),
            b'0' | 0 | b'7' => {
                if let Some(s) = pax_size.take() {
                    size = s;
                }
                let mut name = cstr(&h[0..100]);
                let prefix = cstr(&h[345..500]);
                if !prefix.is_empty() {
                    name = format!("{prefix}/{name}");
                }
                if let Some(n) = long_name.take() {
                    name = n;
                }
                if let Some(n) = pax_path.take() {
                    name = n;
                }
                if data + size > len {
                    bail!("{tar_name}: member {name} runs past the end of the tar");
                }
                if name.ends_with(".xtc") {
                    let label = format!("{tar_name}:{name}");
                    let boxes = walk_xtc(f, data, data + size, &label)?;
                    out.push(Xtc { name: label, boxes });
                }
            }
            _ => {
                // directories, links and the like: nothing of ours
                (long_name, pax_path, pax_size) = (None, None, None);
            }
        }
        pos = data + size.div_ceil(BLOCK) * BLOCK;
    }
    Ok(out)
}

// --------------------------------------------------
/// The XTCs in `path`: the file itself, or each .xtc member of a tar
fn find_xtcs(path: &Path) -> Result<Vec<Xtc>> {
    let mut f = File::open(path).with_context(|| format!("{}", path.display()))?;
    let len = f.metadata()?.len();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut head = [0u8; 512];
    let n = f.read(&mut head)?;
    if n >= 4 && be_i32(&head[0..4]) == XTC_MAGIC {
        let boxes = walk_xtc(&mut f, 0, len, &name)?;
        return Ok(vec![Xtc { name, boxes }]);
    }
    if n == 512 && &head[257..262] == b"ustar" {
        return walk_tar(&mut f, len, &name);
    }
    if len == 0 {
        bail!("{}: empty file", path.display());
    }
    bail!("{}: neither an XTC nor a tar", path.display())
}

fn report(x: &Xtc) -> XtcReport {
    let mut kinds = BTreeMap::new();
    for b in &x.boxes {
        *kinds.entry(box_kind(&b.values())).or_insert(0) += 1;
    }
    XtcReport {
        name: x.name.clone(),
        frames: x.boxes.len(),
        to_zero: x.boxes.iter().filter(|b| !b.is_zero_bytes()).count(),
        kinds,
    }
}

/// Refuse when any frame holds a real-looking cell: that box is data
fn refuse_cells(xtcs: &[Xtc]) -> Result<()> {
    for x in xtcs {
        if let Some((i, b)) = x
            .boxes
            .iter()
            .enumerate()
            .find(|(_, b)| box_kind(&b.values()) == "cell")
        {
            bail!(
                "{}: frame {i} has a real-looking cell {:?}; this simulation has a \
                 box, so it is not zeroed (nothing written)",
                x.name,
                b.values()
            );
        }
    }
    Ok(())
}

// --------------------------------------------------
/// Report on `input` and, with `output` (`--output`), write a copy whose boxes are all
/// zero. Returns the report made BEFORE patching.
pub fn zero_boxes(input: &Path, output: Option<&Path>) -> Result<Vec<XtcReport>> {
    let xtcs = find_xtcs(input)?;
    let reports: Vec<XtcReport> = xtcs.iter().map(report).collect();
    let Some(output) = output else {
        return Ok(reports);
    };
    refuse_cells(&xtcs)?;
    if output.exists() {
        bail!("{}: already exists; not overwritten", output.display());
    }
    let partial = partial_path(output);
    if partial.exists() {
        bail!("{}: left from an earlier run; remove it", partial.display());
    }
    let result = write_and_verify(input, &partial, &xtcs);
    if let Err(e) = result {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    fs::rename(&partial, output).with_context(|| {
        format!("rename {} -> {}", partial.display(), output.display())
    })?;
    Ok(reports)
}

fn partial_path(output: &Path) -> PathBuf {
    let mut s = output.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

fn write_and_verify(input: &Path, partial: &Path, xtcs: &[Xtc]) -> Result<()> {
    fs::copy(input, partial).with_context(|| {
        format!("copy {} -> {}", input.display(), partial.display())
    })?;
    let mut ranges: Vec<(u64, u64)> = vec![];
    {
        let mut w = OpenOptions::new().write(true).open(partial)?;
        for b in xtcs.iter().flat_map(|x| &x.boxes) {
            if !b.is_zero_bytes() {
                w.seek(SeekFrom::Start(b.offset))?;
                w.write_all(&[0u8; BOX_LEN])?;
                ranges.push((b.offset, b.offset + BOX_LEN as u64));
            }
        }
        w.sync_all()?;
    }
    verify(input, partial, xtcs, &ranges)
}

/// The copy holds the same frames, all with zero boxes, and differs from
/// the input only inside the rewritten box fields
fn verify(
    input: &Path,
    partial: &Path,
    before: &[Xtc],
    ranges: &[(u64, u64)],
) -> Result<()> {
    let after = find_xtcs(partial)?;
    if after.len() != before.len() {
        bail!(
            "verify: {} XTCs after, {} before",
            after.len(),
            before.len()
        );
    }
    for (a, b) in after.iter().zip(before) {
        if a.boxes.len() != b.boxes.len() {
            bail!(
                "verify: {}: {} frames after, {} before",
                b.name,
                a.boxes.len(),
                b.boxes.len()
            );
        }
        if let Some(i) = a.boxes.iter().position(|f| !f.is_zero_bytes()) {
            bail!("verify: {}: frame {i} box not zero", b.name);
        }
    }
    let (mut ri, mut ro) = (
        BufReader::with_capacity(1 << 20, File::open(input)?),
        BufReader::with_capacity(1 << 20, File::open(partial)?),
    );
    let (mut bi, mut bo) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    let mut pos = 0u64;
    let mut next_range = 0usize;
    loop {
        let n = ri.read(&mut bi)?;
        let mut got = 0;
        while got < n {
            let m = ro.read(&mut bo[got..n])?;
            if m == 0 {
                bail!("verify: copy is shorter than the input");
            }
            got += m;
        }
        if n == 0 {
            if ro.read(&mut bo[..1])? != 0 {
                bail!("verify: copy is longer than the input");
            }
            return Ok(());
        }
        for i in 0..n {
            if bi[i] != bo[i] {
                let at = pos + i as u64;
                while next_range < ranges.len() && ranges[next_range].1 <= at {
                    next_range += 1;
                }
                let inside = next_range < ranges.len()
                    && ranges[next_range].0 <= at
                    && at < ranges[next_range].1;
                if !inside {
                    bail!("verify: byte {at} changed outside a box field");
                }
            }
        }
        pos += n as u64;
    }
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    type M = [[f32; 3]; 3];
    const CUBE: M = [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 5.0]];
    const ZERO: M = [[0.0; 3]; 3];
    const INF: M = [
        [f32::NEG_INFINITY, 0.0, 0.0],
        [0.0, -214.5, f32::NEG_INFINITY],
        [0.0, 0.0, 0.0],
    ];
    const HUGE: M = [[1.5e10, 0.0, 0.0], [0.0, 41582.0, 0.0], [0.0, 0.0, 3.0]];
    const TINY: M = [[-1.248e-8, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];

    /// An XTC of `natoms` atoms (<= 9 uncompressed, else compressed), one
    /// frame per box, coordinates moving a little each frame
    fn xtc(path: &Path, natoms: usize, boxes: &[M]) -> PathBuf {
        let mut w = molly::XTCWriter::create(path).unwrap();
        for (i, b) in boxes.iter().enumerate() {
            w.write_frame(&molly::Frame {
                step: i as u32,
                time: 10.0 * i as f32,
                boxvec: b
                    .iter()
                    .flatten()
                    .copied()
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                precision: 1000.0,
                positions: (0..natoms * 3)
                    .map(|k| 0.1 * k as f32 + 0.01 * i as f32)
                    .collect(),
            })
            .unwrap();
        }
        path.to_path_buf()
    }

    fn read(path: &Path) -> Vec<molly::Frame> {
        let mut r = molly::XTCReader::open(path).unwrap();
        let mut out = vec![];
        loop {
            let mut f = molly::Frame::default();
            match r.read_frame(&mut f) {
                Ok(()) => out.push(f),
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => panic!("{}: {e}", path.display()),
            }
        }
        out
    }

    fn same_coordinates(a: &Path, b: &Path) {
        let (fa, fb) = (read(a), read(b));
        assert_eq!(fa.len(), fb.len());
        for (x, y) in fa.iter().zip(&fb) {
            assert_eq!(x.positions, y.positions);
            assert_eq!(x.step, y.step);
            assert_eq!(x.time, y.time);
            assert_eq!(x.precision, y.precision);
        }
    }

    fn all_zero_boxes(p: &Path) {
        let frames = read(p);
        assert!(!frames.is_empty(), "{}", p.display());
        for f in frames {
            let b = f.boxvec_cols_2d();
            assert!(b.iter().flatten().all(|&v| v == 0.0), "{b:?}");
        }
    }

    /// A tar of (member name, file) pairs, written by the `tar` crate
    fn tar_of(path: &Path, members: &[(&str, &Path)]) -> PathBuf {
        let mut b = tar::Builder::new(File::create(path).unwrap());
        for (name, file) in members {
            b.append_path_with_name(file, name).unwrap();
        }
        b.finish().unwrap();
        path.to_path_buf()
    }

    fn untar(tarfile: &Path, dir: &Path) {
        tar::Archive::new(File::open(tarfile).unwrap())
            .unpack(dir)
            .unwrap();
    }

    #[test]
    fn infinite_boxes_become_zero_coordinates_untouched_compressed() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF, INF, INF]);
        let o = d.path().join("out.xtc");
        let r = zero_boxes(&i, Some(&o)).unwrap();
        assert_eq!((r[0].frames, r[0].to_zero), (3, 3));
        assert_eq!(r[0].kinds["non-finite"], 3);
        all_zero_boxes(&o);
        same_coordinates(&i, &o);
        assert_eq!(
            fs::metadata(&i).unwrap().len(),
            fs::metadata(&o).unwrap().len()
        );
    }

    #[test]
    fn uncompressed_frames_too() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 3, &[HUGE, INF]);
        let o = d.path().join("out.xtc");
        let r = zero_boxes(&i, Some(&o)).unwrap();
        assert_eq!((r[0].frames, r[0].to_zero), (2, 2));
        all_zero_boxes(&o);
        same_coordinates(&i, &o);
    }

    #[test]
    fn nine_and_ten_atoms_both_walk() {
        // 9 is the last uncompressed count, 10 the first compressed one
        for n in [9, 10] {
            let d = tempfile::tempdir().unwrap();
            let i = xtc(&d.path().join("in.xtc"), n, &[INF, HUGE, ZERO, INF]);
            let o = d.path().join("out.xtc");
            let r = zero_boxes(&i, Some(&o)).unwrap();
            assert_eq!((r[0].frames, r[0].to_zero), (4, 3), "natoms {n}");
            all_zero_boxes(&o);
            same_coordinates(&i, &o);
        }
    }

    #[test]
    fn frame_count_matches_molly() {
        let d = tempfile::tempdir().unwrap();
        let boxes: Vec<M> = (0..57)
            .map(|k| if k % 3 == 0 { INF } else { ZERO })
            .collect();
        let i = xtc(&d.path().join("in.xtc"), 500, &boxes);
        let r = zero_boxes(&i, None).unwrap();
        assert_eq!(r[0].frames, read(&i).len());
        assert_eq!(r[0].to_zero, 19);
    }

    #[test]
    fn only_box_bytes_differ() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 40, &[INF, ZERO, HUGE]);
        let o = d.path().join("out.xtc");
        zero_boxes(&i, Some(&o)).unwrap();
        let (a, b) = (fs::read(&i).unwrap(), fs::read(&o).unwrap());
        let boxes = find_xtcs(&i).unwrap()[0].boxes.clone();
        for (k, (x, y)) in a.iter().zip(&b).enumerate() {
            if x != y {
                let k = k as u64;
                assert!(
                    boxes.iter().any(|f| f.offset <= k && k < f.offset + 36),
                    "byte {k}"
                );
            }
        }
        assert_ne!(a, b);
    }

    #[test]
    fn tiny_nonzero_box_is_zeroed_too() {
        // 105222 carried -1.248e-8: "zero" to the checks, not zero bytes
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[TINY]);
        let o = d.path().join("out.xtc");
        let r = zero_boxes(&i, Some(&o)).unwrap();
        assert_eq!(r[0].kinds["zero"], 1);
        assert_eq!(r[0].to_zero, 1);
        all_zero_boxes(&o);
    }

    #[test]
    fn an_already_zero_file_is_copied_unchanged() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[ZERO, ZERO]);
        let o = d.path().join("out.xtc");
        let r = zero_boxes(&i, Some(&o)).unwrap();
        assert_eq!(r[0].to_zero, 0);
        assert_eq!(fs::read(&i).unwrap(), fs::read(&o).unwrap());
    }

    #[test]
    fn a_real_cell_is_refused_and_nothing_written() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF, CUBE]);
        let o = d.path().join("out.xtc");
        let e = zero_boxes(&i, Some(&o)).unwrap_err().to_string();
        assert!(
            e.contains("frame 1") && e.contains("real-looking cell"),
            "{e}"
        );
        assert!(!o.exists());
        assert!(!partial_path(&o).exists());
    }

    #[test]
    fn report_only_writes_nothing_and_allows_cells() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[CUBE, INF]);
        let before = fs::read(&i).unwrap();
        let r = zero_boxes(&i, None).unwrap();
        assert_eq!(r[0].kinds["cell"], 1);
        assert_eq!(r[0].kinds["non-finite"], 1);
        assert_eq!(fs::read(&i).unwrap(), before);
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn an_existing_output_is_not_overwritten() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF]);
        let o = d.path().join("out.xtc");
        fs::write(&o, "keep me").unwrap();
        assert!(zero_boxes(&i, Some(&o)).is_err());
        assert_eq!(fs::read_to_string(&o).unwrap(), "keep me");
    }

    #[test]
    fn a_leftover_partial_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF]);
        let o = d.path().join("out.xtc");
        fs::write(partial_path(&o), "old").unwrap();
        let e = zero_boxes(&i, Some(&o)).unwrap_err().to_string();
        assert!(e.contains("earlier run"), "{e}");
        assert!(!o.exists());
    }

    #[test]
    fn output_onto_input_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF]);
        let before = fs::read(&i).unwrap();
        assert!(zero_boxes(&i, Some(&i)).is_err());
        assert_eq!(fs::read(&i).unwrap(), before);
    }

    #[test]
    fn a_truncated_xtc_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF, INF]);
        let mut b = fs::read(&i).unwrap();
        b.truncate(b.len() - 7);
        fs::write(&i, &b).unwrap();
        let e = zero_boxes(&i, None).unwrap_err().to_string();
        assert!(e.contains("frame 1"), "{e}");
    }

    #[test]
    fn a_damaged_atom_count_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        let i = xtc(&d.path().join("in.xtc"), 12, &[INF]);
        let mut b = fs::read(&i).unwrap();
        b[55] ^= 1; // second natoms
        fs::write(&i, &b).unwrap();
        let e = zero_boxes(&i, None).unwrap_err().to_string();
        assert!(e.contains("atom counts"), "{e}");
    }

    #[test]
    fn not_an_xtc_or_tar_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.pdb");
        fs::write(&p, "ATOM      1  N   MET A   1\n").unwrap();
        let e = zero_boxes(&p, None).unwrap_err().to_string();
        assert!(e.contains("neither"), "{e}");
        let empty = d.path().join("e.xtc");
        fs::write(&empty, "").unwrap();
        assert!(zero_boxes(&empty, None).is_err());
    }

    #[test]
    fn tar_members_are_patched_in_place() {
        let d = tempfile::tempdir().unwrap();
        let bad = xtc(&d.path().join("a.xtc"), 12, &[INF, INF]);
        let good = xtc(&d.path().join("b.xtc"), 12, &[ZERO, ZERO]);
        let huge = xtc(&d.path().join("c.xtc"), 5, &[HUGE]);
        let note = d.path().join("notes.txt");
        fs::write(&note, "not a trajectory").unwrap();
        let t = tar_of(
            &d.path().join("full.tar"),
            &[
                ("full/rep1.xtc", bad.as_path()),
                ("full/notes.txt", note.as_path()),
                ("full/rep2.xtc", good.as_path()),
                ("full/rep3.xtc", huge.as_path()),
            ],
        );
        let o = d.path().join("out.tar");
        let r = zero_boxes(&t, Some(&o)).unwrap();
        let by: BTreeMap<_, _> =
            r.iter().map(|x| (x.name.as_str(), x.to_zero)).collect();
        assert_eq!(
            by,
            BTreeMap::from([
                ("full.tar:full/rep1.xtc", 2),
                ("full.tar:full/rep2.xtc", 0),
                ("full.tar:full/rep3.xtc", 1)
            ])
        );
        assert_eq!(
            fs::metadata(&t).unwrap().len(),
            fs::metadata(&o).unwrap().len()
        );
        let x = d.path().join("x");
        untar(&o, &x);
        for (n, src) in [("rep1", &bad), ("rep2", &good), ("rep3", &huge)] {
            let p = x.join(format!("full/{n}.xtc"));
            all_zero_boxes(&p);
            same_coordinates(src, &p);
        }
        assert_eq!(
            fs::read_to_string(x.join("full/notes.txt")).unwrap(),
            "not a trajectory"
        );
    }

    #[test]
    fn a_long_member_name_is_followed() {
        // over 100 characters: GNU long-name or pax, depending on the writer
        let d = tempfile::tempdir().unwrap();
        let bad = xtc(&d.path().join("a.xtc"), 12, &[INF]);
        let long = format!(
            "full/{}-replicate10.xtc",
            "I799T-EHMT1-ankyrin-model-with-peptide".repeat(3)
        );
        let t = tar_of(
            &d.path().join("full.tar"),
            &[(long.as_str(), bad.as_path())],
        );
        let r = zero_boxes(&t, None).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, format!("full.tar:{long}"));
        let o = d.path().join("out.tar");
        zero_boxes(&t, Some(&o)).unwrap();
        let x = d.path().join("x");
        untar(&o, &x);
        all_zero_boxes(&x.join(&long));
    }

    #[test]
    fn a_tar_with_a_real_cell_in_one_member_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let bad = xtc(&d.path().join("a.xtc"), 12, &[INF]);
        let cell = xtc(&d.path().join("b.xtc"), 12, &[CUBE]);
        let t = tar_of(
            &d.path().join("full.tar"),
            &[("rep1.xtc", bad.as_path()), ("rep2.xtc", cell.as_path())],
        );
        let o = d.path().join("out.tar");
        let e = zero_boxes(&t, Some(&o)).unwrap_err().to_string();
        assert!(e.contains("rep2.xtc"), "{e}");
        assert!(!o.exists());
    }

    #[test]
    fn a_pax_path_names_the_member() {
        // A pax 'x' header whose path= replaces the member header's name
        let d = tempfile::tempdir().unwrap();
        let bad = xtc(&d.path().join("a.xtc"), 12, &[INF, HUGE]);
        let data = fs::read(&bad).unwrap();
        let long = format!("minimal/{}.xtc", "x".repeat(150));
        let body = format!(" path={long}\n");
        let mut n = body.len() + 1;
        while format!("{n}{body}").len() != n {
            n += 1;
        }
        let pax = format!("{n}{body}");
        let t = d.path().join("minimal.tar");
        let mut b = tar::Builder::new(File::create(&t).unwrap());
        let mut h = tar::Header::new_ustar();
        h.set_entry_type(tar::EntryType::XHeader);
        h.set_path("PaxHeader/x").unwrap();
        h.set_size(pax.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append(&h, pax.as_bytes()).unwrap();
        let mut h = tar::Header::new_ustar();
        h.set_entry_type(tar::EntryType::Regular);
        h.set_path("short-stand-in.xtc").unwrap();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append(&h, data.as_slice()).unwrap();
        b.finish().unwrap();
        drop(b);
        let r = zero_boxes(&t, None).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, format!("minimal.tar:{long}"));
        assert_eq!(r[0].to_zero, 2);
        let o = d.path().join("out.tar");
        zero_boxes(&t, Some(&o)).unwrap();
        let x = d.path().join("x");
        untar(&o, &x);
        all_zero_boxes(&x.join(&long));
        same_coordinates(&bad, &x.join(&long));
    }

    #[test]
    fn a_damaged_tar_header_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        let bad = xtc(&d.path().join("a.xtc"), 12, &[INF]);
        let t = tar_of(&d.path().join("full.tar"), &[("rep1.xtc", bad.as_path())]);
        let mut b = fs::read(&t).unwrap();
        b[10] ^= 0x20; // inside the name: checksum no longer matches
        fs::write(&t, &b).unwrap();
        let e = zero_boxes(&t, None).unwrap_err().to_string();
        assert!(e.contains("checksum"), "{e}");
    }

    #[test]
    fn an_empty_tar_reports_nothing() {
        let d = tempfile::tempdir().unwrap();
        let note = d.path().join("n.txt");
        fs::write(&note, "x").unwrap();
        let t = tar_of(&d.path().join("t.tar"), &[("n.txt", note.as_path())]);
        assert!(zero_boxes(&t, None).unwrap().is_empty());
    }

    #[test]
    fn tar_numbers() {
        assert_eq!(tar_number(b"00000001750\0").unwrap(), 0o1750);
        assert_eq!(tar_number(b"     12 \0\0\0\0").unwrap(), 0o12);
        assert_eq!(tar_number(b"\0\0\0\0").unwrap(), 0);
        // base-256 (GNU) for sizes over 8 GB: top bit set, big-endian after
        let n: u64 = 23_000_000_000;
        let mut big = [0u8; 12];
        big[4..].copy_from_slice(&n.to_be_bytes());
        big[0] = 0x80;
        assert_eq!(tar_number(&big).unwrap(), n);
    }

    #[test]
    fn pax_records_parse() {
        // Each record is "LEN key=value\n", LEN counting the whole record
        fn record(kv: &str) -> String {
            let body = format!(" {kv}\n");
            let mut len = body.len() + 1;
            while format!("{len}{body}").len() != len {
                len += 1;
            }
            format!("{len}{body}")
        }
        let rec = [
            record("path=full/a-very-long.xtc"),
            record("size=123456"),
            record("mtime=1700000000"),
        ]
        .concat();
        assert!(rec.starts_with("29 path="), "{rec}");
        assert_eq!(
            pax_records(rec.as_bytes()),
            (Some("full/a-very-long.xtc".into()), Some(123456))
        );
        // Only one of the two keys
        assert_eq!(pax_records(record("size=42").as_bytes()), (None, Some(42)));
        // Not records at all, or a length past the end: nothing, no panic
        assert_eq!(pax_records(b"garbage"), (None, None));
        assert_eq!(pax_records(b"99 path=x\n"), (None, None));
    }

    /// Real replicates, when present (the ddd host): every .xtc or .tar in
    /// MDR_ZERO_BOX_FILES (colon-separated) is patched to a temp copy and
    /// must come out all-zero with molly reading the same coordinates.
    /// MDR_ZERO_BOX_FILES=a.xtc:b.tar cargo test -p mdr-process -- --ignored real_files
    #[test]
    #[ignore = "needs files in MDR_ZERO_BOX_FILES"]
    fn real_files_zero() {
        let files = std::env::var("MDR_ZERO_BOX_FILES").expect("MDR_ZERO_BOX_FILES");
        for f in files.split(':') {
            let d = tempfile::tempdir().unwrap();
            let i = Path::new(f);
            let o = d.path().join(i.file_name().unwrap());
            let r = zero_boxes(i, Some(&o)).unwrap();
            assert!(!r.is_empty(), "{f}");
            if f.ends_with(".xtc") {
                all_zero_boxes(&o);
                same_coordinates(i, &o);
            } else {
                let x = d.path().join("x");
                untar(&o, &x);
                let y = d.path().join("y");
                untar(i, &y);
                for rep in &r {
                    let member = rep.name.split_once(':').unwrap().1;
                    all_zero_boxes(&x.join(member));
                    same_coordinates(&y.join(member), &x.join(member));
                }
            }
        }
    }
}
