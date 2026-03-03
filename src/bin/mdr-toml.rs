use anyhow::Result;
use clap::Parser;
use mdr::metadata::Meta;
use std::path::PathBuf;

// --------------------------------------------------
#[derive(Parser, Debug)]
pub struct Args {
    #[arg()]
    pub filename: PathBuf,
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
    let meta = Meta::from_file(&args.filename)?;
    println!("{}", meta.check().join("\n"));
    Ok(())
}
