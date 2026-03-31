pub mod models;
pub mod ops;
pub mod schema;

use diesel::pg::PgConnection;
use diesel::prelude::*;
//use dotenvy::dotenv;
//use std::env;

/// Establish a direct `PgConnection` (used by the CLI).
pub fn connect(url: &str) -> PgConnection {
    //dotenv().ok();
    //let url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(url).unwrap_or_else(|e| panic!("Error connecting: {e}"))
}
