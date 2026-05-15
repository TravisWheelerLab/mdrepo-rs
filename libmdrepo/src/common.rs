use anyhow::{anyhow, bail, Result};
use lazy_regex::regex;
use log::debug;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    process::Command,
};
use which::which;

// --------------------------------------------------
pub fn file_exists(file: &Path) -> bool {
    if let Ok(meta) = fs::metadata(file) {
        meta.is_file() && meta.len() > 0
    } else {
        false
    }
}

// --------------------------------------------------
pub fn read_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| anyhow!("{}: {e}", path.display()))
}

// --------------------------------------------------
pub fn get_md5(path: &Path) -> Result<String> {
    debug!("Getting MD5 '{}'", path.display());
    let input_dir = path
        .parent()
        .ok_or_else(|| anyhow!("No parent directory for '{}'", path.display()))?;
    let filename = path
        .file_name()
        .ok_or_else(|| anyhow!("No filename for '{}'", path.display()))?
        .to_string_lossy()
        .to_string();
    let md5_file = input_dir.join(format!("{filename}.md5"));

    if !file_exists(&md5_file) {
        let md5_prg = which("md5sum")?;
        let cmd = Command::new(&md5_prg).arg(path).output()?;
        if !cmd.status.success() {
            bail!(str::from_utf8(&cmd.stderr)?.to_string());
        }
        let stdout = str::from_utf8(&cmd.stdout)?.trim_end();
        let re = regex!(r"^([a-f0-9]{32})\s+");
        let caps = re
            .captures(stdout)
            .ok_or(anyhow!(r#"Unexpected MD5: {stdout}"#))?;
        let digest = &caps[1];
        let out_fh = File::create(&md5_file)?;
        write!(&out_fh, "{}", digest)?;
    }

    read_file(&md5_file)
}

// --------------------------------------------------
pub fn get_simulation_id(val: &str) -> Result<u64> {
    regex!(r"^(?:MDR)?0*([1-9][0-9]*)$")
        .captures(val.trim())
        .and_then(|caps| caps[1].parse::<u64>().ok())
        .ok_or_else(|| anyhow!(r#"Invalid simulation ID "{}""#, val))
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn test_get_simulation_id() {
        let res = get_simulation_id("");
        assert!(res.is_err());

        let res = get_simulation_id("MDR");
        assert!(res.is_err());

        let res = get_simulation_id("0");
        assert!(res.is_err());

        let res = get_simulation_id("MDR000000000");
        assert!(res.is_err());

        let res = get_simulation_id("564");
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, 564);

        let res = get_simulation_id("000564");
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, 564);

        let res = get_simulation_id("MDR00000564");
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, 564);

        let res = get_simulation_id("MDR56400001");
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, 56400001);

        let res = get_simulation_id("\tMDR56400001 ");
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, 56400001);
    }

    #[test]
    fn test_file_exists() {
        assert!(!file_exists(&Path::new("blargh")));
        assert!(file_exists(&Path::new(
            "tests/inputs/metadata/MDR00015378.v1.toml"
        )));
    }

    #[test]
    fn test_read_file() {
        let res = read_file(&Path::new("blargh"));
        assert!(res.is_err());

        let res = read_file(&Path::new("tests/inputs/metadata/empty.txt"));
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "".to_string());
    }

    #[test]
    fn test_get_md5() {
        let res = get_md5(&Path::new("blargh"));
        assert!(res.is_err());

        let filename = "tests/inputs/metadata/MDR00015378.v1.toml";
        let cached = format!("{filename}.md5");
        if Path::new(&cached).exists() {
            let _ = fs::remove_file(&cached);
        }

        let res = get_md5(&Path::new(filename));
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "cd862822bcf0881ed9f0d94ad01dda3f".to_string());

        if Path::new(&cached).exists() {
            let _ = fs::remove_file(&cached);
        }
    }
}
