use clap::{builder::PossibleValue, Parser, ValueEnum};
use std::fmt;

#[derive(Debug, Parser)]
pub struct Args {
    /// Simulation ID
    #[arg(short, long, value_name = "SIM_ID", required = true)]
    pub simulation_id: i64,

    /// Server
    #[arg(short('S'), long, value_name = "SERVER", default_value = "staging")]
    pub server: Server,

    /// Database DSN
    #[arg(short, long, value_name = "DSN")]
    pub dsn: Option<String>,

    /// Output filename ("-" for STDOUT)
    #[arg(short, long, value_name = "FILE", default_value = "-")]
    pub outfile: String,

    /// Output format
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<FileFormat>,
}

// --------------------------------------------------
#[derive(Debug, PartialEq, Clone)]
pub enum FileFormat {
    Json,
    Toml,
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

// --------------------------------------------------
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
