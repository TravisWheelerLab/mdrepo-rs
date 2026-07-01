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
        })
        .target(match args.log_file {
            // Optional log file, default to STDOUT
            Some(ref filename) => env_logger::Target::Pipe(Box::new(BufWriter::new(
                File::create(filename)?,
            ))),
            _ => env_logger::Target::Stdout,
        })
        .init();

    let num_threads = args.num_threads.unwrap_or(num_cpus::get());
    info!(
        "Using {num_threads} thread{}",
        if num_threads == 1 { "" } else { "s" }
    );
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .unwrap();

    match &args.command {
        Command::Process(args) => {
            let errors = process::process(args)?;
            if !errors.is_empty() {
                info!("Errors:\n{}", errors.join("\n"));
            }
            info!("Finished");
            Ok(())
        }
        Command::Reprocess(args) => {
            let errors = reprocess::reprocess(args)?;
            if !errors.is_empty() {
                info!("Errors:\n{}", errors.join("\n"));
            }
            info!("Finished");
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
                Err(e) => {
                    vec![format!("Unable to parse {}: {e}", args.filename.display())]
                }
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
                Ok(()) => info!("Finished"),
            }
            Ok(())
        }
        Command::Validate(args) => {
            for (dir_num, dirname) in args.dirnames.iter().enumerate() {
                print!(
                    "{}{}: ",
                    if dir_num > 0 { "\n" } else { "" },
                    dirname.display()
                );

                let opts = args.no_id.then_some(MetaCheckOptions {
                    allow_no_pdb_uniprot: true,
                });
                match validate::validate(dirname, opts) {
                    Err(e) => bail!("{e}"),
                    Ok(errors) => {
                        if errors.is_empty() {
                            println!("OK");
                        } else {
                            let num_errors = errors.len();
                            println!(
                                "{num_errors} error{}",
                                if num_errors == 1 { "" } else { "s" }
                            );
                            for error in errors {
                                println!("{error}");
                            }
                        }
                    }
                }
            }
            Ok(())
        }
    }
}
