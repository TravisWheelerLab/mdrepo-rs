use anyhow::{bail, Result};
use clap::Parser;
use libmdrepo::{metadata::Meta, metadatav1::MetaV1};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

#[derive(Parser, Debug)]
/// Convert MDRepo TOML v1 to v2
pub struct Args {
    /// Input TOML file
    #[arg(value_name = "FILE")]
    pub filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT")]
    outfile: Option<String>,

    /// Output filename
    #[arg(short, long)]
    in_place: bool,
}

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Args) -> Result<()> {
    if args.filename == args.outfile.clone().unwrap_or("".to_string()) {
        bail!("Will not overwrite input file with output file")
    }

    if Meta::from_file(Path::new(&args.filename)).is_ok() {
        bail!(r#""{}" is already in v2 format"#, &args.filename);
    }

    let v1 = MetaV1::from_file(Path::new(&args.filename))?;
    let meta = v1.to_v2()?;

    let outfile = if let Some(output) = args.outfile {
        output
    } else if args.in_place {
        let input = args.filename;
        let backup = format!("{input}.bak");
        if Path::new(&backup).exists() {
            bail!(r#"Backup file "{backup}" already exists"#);
        }
        fs::copy(&input, &backup)?;
        input
    } else {
        "-".to_string()
    };

    let mut out_fh = open_outfile(&outfile)?;
    write!(out_fh, "{}", meta.to_toml()?)?;
    Ok(())
}

// --------------------------------------------------
fn open_outfile(filename: &str) -> Result<Box<dyn Write>> {
    match filename {
        "-" => Ok(Box::new(io::stdout())),
        out_name => Ok(Box::new(File::create(out_name)?)),
    }
}
