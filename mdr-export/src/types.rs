use clap::{Parser, ValueEnum, builder::PossibleValue};
use std::fmt;

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
    pub out_dir: String,
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
