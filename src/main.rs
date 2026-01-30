mod constants;
mod metadata;
mod types;
mod validate;

use anyhow::Result;
use clap::Parser;
use metadata::Meta;
//use log::info;
use std::{
    fs::{self, File},
    io::BufWriter,
};
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
        Some(Command::SimProc(args)) => {
            dbg!(&args);
            //let mut out_file = open_outfile(&args.outfile)?;
            //let meta = Meta::example();
            //write!(
            //    out_file,
            //    "{}",
            //    if args.format == FileFormat::Json {
            //        meta.to_json()?
            //    } else {
            //        meta.to_toml()?
            //    }
            //)?;
            Ok(())
        }
        Some(Command::MetaCheck(args)) => {
            dbg!(&args);
            let toml = fs::read_to_string(&args.filename)?;
            let mut meta: Meta = toml::from_str(&toml)?;
            meta.fix();
            dbg!(&meta);
            Ok(())
        }
        Some(Command::Validate(args)) => {
            validate::validate(&args)?;
            Ok(())
        }
        None => unreachable!(),
    }
}
