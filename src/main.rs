use anyhow::Result;
use clap::Parser;
use mdr::{
    metadata::Meta,
    process, reprocess, ticket,
    types::{Cli, Command, LogLevel},
    validate,
};
use std::{fs::File, io::BufWriter};
//use validator::Validate;

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
            process::process(args)?;
            Ok(())
        }
        Some(Command::Reprocess(args)) => {
            reprocess::reprocess(args)?;
            Ok(())
        }
        Some(Command::MetaCheck(args)) => {
            let messages = match Meta::from_file(&args.filename) {
                Ok(meta) => meta.check(),
                Err(e) => vec![format!(
                    "Unable to parse {}: {}",
                    args.filename.display(),
                    e.to_string()
                )],
                //Ok(meta) => match meta.validate() {
                //    Ok(()) => vec![],
                //    Err(errors) => {
                //        dbg!(&errors);
                //        let mut ret = vec![];
                //        for (field, _val) in errors.errors() {
                //            ret.push(format!("{field}"));
                //        }
                //        ret
                //    }
                //},
            };

            println!("{}", messages.join("\n"));
            Ok(())
        }
        Some(Command::Ticket(args)) => {
            ticket::process(args)?;
            Ok(())
        }
        Some(Command::Validate(args)) => {
            validate::validate(&args.dirname)?;
            Ok(())
        }
        None => unreachable!(),
    }
}
