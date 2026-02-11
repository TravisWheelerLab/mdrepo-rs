mod common;
mod constants;
mod metadata;
mod process;
mod types;
mod validate;

use anyhow::Result;
use clap::Parser;
//use diesel::prelude::*;
use metadata::Meta;
use std::{fs::File, io::BufWriter};
use types::{Cli, Command, LogLevel};

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Cli) -> Result<()> {
    env_logger::Builder::new()
        .filter_level(match args.log {
            Some(LogLevel::Debug) => log::LevelFilter::Debug,
            Some(LogLevel::Info) => log::LevelFilter::Info,
            _ => log::LevelFilter::Off,
        })
        .target(match args.log_file {
            // Optional log file, default to STDOUT
            Some(ref filename) => env_logger::Target::Pipe(Box::new(BufWriter::new(
                File::create(filename)?,
            ))),
            _ => env_logger::Target::Stdout,
        })
        .init();

    match &args.command {
        Some(Command::Process(args)) => {
            //validate::validate(&args.dirname)?;
            process::process(&args)?;
            Ok(())
        }
        //Some(Command::Reprocess(args)) => {
        //    process::process(&args)?;
        //    Ok(())
        //}
        Some(Command::MetaCheck(args)) => {
            let meta = Meta::from_file(&args.filename)?;
            dbg!(&meta);
            Ok(())
        }
        Some(Command::Validate(args)) => {
            validate::validate(&args.dirname)?;
            Ok(())
        }
        None => unreachable!(),
    }
}
