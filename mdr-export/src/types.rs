use clap::{Parser, ValueEnum, builder::PossibleValue};
use std::{fmt, path::PathBuf};

#[derive(Debug, Parser)]
pub struct Args {
    /// Simulation IDs
    #[arg(short('i'), long, value_name = "ID", num_args = 0..)]
    pub simulation_ids: Vec<i64>,

    /// Server
    #[arg(short, long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,

    /// Output directory
    #[arg(short, long, value_name = "DIR", default_value = "export")]
    pub out_dir: PathBuf,

    /// Output format
    #[arg(short, long, value_name = "FORMAT", default_value = "toml")]
    pub format: FileFormat,

    /// Force overwrite of existing files
    #[arg(short('O'), long)]
    pub overwrite: bool,
}

// --------------------------------------------------
#[derive(Debug, PartialEq, Clone)]
pub enum FileFormat {
    Json,
    Toml,
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FileFormat::Json => "json",
                FileFormat::Toml => "toml",
            }
        )
    }
}

impl ValueEnum for FileFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[FileFormat::Json, FileFormat::Toml]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            FileFormat::Json => PossibleValue::new("json"),
            FileFormat::Toml => PossibleValue::new("toml"),
        })
    }
}

#[derive(Debug, Clone)]
pub enum Server {
    Production,
    Staging,
}

impl ValueEnum for Server {
    fn value_variants<'a>() -> &'a [Self] {
        &[Server::Production, Server::Staging]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            Server::Production => PossibleValue::new("prod"),
            Server::Staging => PossibleValue::new("staging"),
        })
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Server::Production => "prod",
                Server::Staging => "staging",
            }
        )
    }
}
