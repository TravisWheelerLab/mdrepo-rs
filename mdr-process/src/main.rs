use anyhow::{Result, anyhow, bail};
use clap::Parser;
use libmdrepo::metadata::{Meta, MetaCheckOptions};
use log::info;
use mdr_process::{
    process, reprocess, ticket,
    types::{Cli, Command, LogLevel},
    validate,
};
use std::{env, fs::File, io::BufWriter, path::PathBuf};
use which::which;

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
            // Processing rewrites the metadata TOML in place, so verify the
            // upload against its manifest first or there is nothing left to
            // verify it against. `ticket` runs this itself before calling
            // `process`; a directory that did not come from a ticket has no
            // manifest and is checked on its metadata alone.
            if args.input_dir.join(ticket::COMPLETED_JSON).is_file() {
                let errors = ticket::check_manifest(&args.input_dir)?;
                if !errors.is_empty() {
                    bail!("Upload is incomplete or corrupt:\n{}", errors.join("\n"));
                }
            }

            process::process(args)?;
            info!("Finished");
            Ok(())
        }
        Command::Reprocess(args) => {
            reprocess::reprocess(args)?;
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
            // Canonicalize ligand SMILES exactly as processing does, so a
            // hand-check judges the forms the validator will actually see.
            let script_dir = PathBuf::from(
                env::var("SCRIPT_DIR").map_err(|e| anyhow!("SCRIPT_DIR: {e}"))?,
            );
            let uv = which("uv").map_err(|e| anyhow!("Failed to find uv ({e})"))?;

            for (dir_num, dirname) in args.dirnames.iter().enumerate() {
                print!(
                    "{}{}: ",
                    if dir_num > 0 { "\n" } else { "" },
                    dirname.display()
                );

                let opts = args.no_id.then_some(MetaCheckOptions {
                    allow_no_pdb_uniprot: true,
                });

                // Check the upload against its manifest when there is one; a
                // directory that did not come from a ticket has none.
                let mut errors = vec![];
                if dirname.join(ticket::COMPLETED_JSON).is_file() {
                    match ticket::check_manifest(dirname) {
                        Err(e) => errors.push(e.to_string()),
                        Ok(upload_errors) => errors.extend(upload_errors),
                    }
                }

                let meta_path = dirname.join("mdrepo-metadata.toml");
                let meta_errors =
                    validate::load_canonical_meta(&meta_path, &script_dir, &uv)
                        .and_then(|meta| validate::validate_meta(dirname, &meta, opts));
                if let Ok(ref meta_errors) = meta_errors {
                    errors.extend(meta_errors.iter().cloned());
                }

                if errors.is_empty() && meta_errors.is_ok() {
                    println!("OK");
                } else {
                    let num_errors = errors.len() + usize::from(meta_errors.is_err());
                    println!(
                        "{num_errors} error{}",
                        if num_errors == 1 { "" } else { "s" }
                    );
                    for error in errors {
                        println!("{error}");
                    }
                }

                // Report the upload errors above before failing, so that an
                // unparsable TOML cannot mask a faulty download.
                if let Err(e) = meta_errors {
                    println!("{e}");
                    bail!("Metadata check failed for {}", dirname.display());
                }
            }
            Ok(())
        }
    }
}
