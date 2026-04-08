use anyhow::{anyhow, bail, Result};
use log::info;
use regex::Regex;
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use which::which;

// --------------------------------------------------
pub fn file_exists(file: &PathBuf) -> bool {
    if let Ok(meta) = fs::metadata(file) {
        meta.is_file() && meta.len() > 0
    } else {
        false
    }
}

// --------------------------------------------------
pub fn read_file(path: &PathBuf) -> Result<String> {
    fs::read_to_string(path).map_err(|e| anyhow!("{}: {e}", path.display()))
}

// --------------------------------------------------
pub fn get_md5(path: &Path) -> Result<String> {
    info!("Getting MD5 '{}'", path.display());
    let input_dir = path.parent().expect("parent_dir");
    let filename = path
        .file_name()
        .expect("filename")
        .to_string_lossy()
        .to_string();
    let md5_file = input_dir.join(format!("{filename}.md5"));

    if !file_exists(&md5_file) {
        let md5_prg = which("md5sum")?;
        let cmd = Command::new(&md5_prg).arg(path).output()?;
        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }
        let stdout = str::from_utf8(&cmd.stdout)?.to_string();
        let stdout = stdout.trim_end();
        let re = Regex::new(r"^([a-f0-9]{32})\s+").unwrap();
        let caps = re
            .captures(stdout)
            .ok_or(anyhow!(r#"Unexpected MD5: {stdout}"#))?;
        if let Some(digest) = caps.get(1) {
            let out_fh = File::create(&md5_file)?;
            write!(&out_fh, "{}", digest.as_str())?;
        }
    }

    read_file(&md5_file)
}
