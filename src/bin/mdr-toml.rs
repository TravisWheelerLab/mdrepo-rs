use anyhow::{anyhow, Result};
use clap::Parser;
use mdr::metadata::Meta;
use std::{fs, io};

// --------------------------------------------------
#[derive(Parser, Debug)]
pub struct Args {
    /// Input filename or "-" for STDIN
    #[arg()]
    pub filename: String,
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
    let contents = open(&args.filename)?;
    let meta = Meta::from_string(&contents)?;
    println!("{}", meta.check().join("\n"));
    Ok(())
}

// --------------------------------------------------
fn open(filename: &str) -> Result<String> {
    match filename {
        "-" => {
            let mut contents = vec![];
            for line in io::stdin().lines() {
                contents.push(line.unwrap());
            }
            Ok(contents.join("\n"))
        }
        _ => fs::read_to_string(filename)
            .map_err(|e| anyhow!(format!("{filename}: {e}"))),
    }
}
