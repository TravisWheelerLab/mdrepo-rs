use anyhow::{bail, Result};
use clap::Parser;
use libmdrepo::metadata::{Meta, MetaCheckOptions};
use log::info;
use mdr_process::{
    process, reprocess, ticket,
    types::{Cli, Command, LogLevel},
    validate,
};
use std::{fs::File, io::BufWriter};

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Cli) -> Result<()> {
    env_logger::Builder::new()
        .filter_level(match args.log {
            Some(LogLevel::Debug) => log::LevelFilter::Debug,
            _ => log::LevelFilter::Info,
            //Some(LogLevel::Info) => log::LevelFilter::Info,
            //_ => log::LevelFilter::Off,
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
        Command::Process(args) => {
            validate::validate(&args.input_dir)?;
            match process::process(args) {
                Err(e) => info!("Error: {e}"),
                _ => info!("Success"),
            }
            Ok(())
        }
        Command::Reprocess(args) => {
            match reprocess::reprocess(args) {
                Err(e) => info!("Error: {e}"),
                _ => info!("Success"),
            }
            Ok(())
        }
        Command::MetaCheck(args) => {
            let messages = match Meta::from_file(&args.filename) {
                Ok(meta) => {
                    let opts = if args.no_id {
                        Some(MetaCheckOptions {
                            allow_no_pdb_uniprot: true,
                        })
                    } else {
                        None
                    };
                    meta.check(opts)
                }
                Err(e) => vec![format!(
                    "Unable to parse {}: {}",
                    args.filename.display(),
                    e.to_string()
                )],
            };

            if !messages.is_empty() {
                println!("{}", messages.join("\n"));
            }

            Ok(())
        }
        Command::Ticket(args) => {
            match ticket::process(args) {
                Err(e) => {
                    let message = match ticket::get_ticket_user(args) {
                        Ok(ticket) => format!("{e}\nNotify User\n{ticket:#?}"),
                        Err(e2) => format!("{e} ({e2})"),
                    };
                    bail!(message);
                }
                _ => info!("Success"),
            }
            Ok(())
        }
        Command::Validate(args) => {
            validate::validate(&args.dirname)?;
            Ok(())
        }
    }
}
