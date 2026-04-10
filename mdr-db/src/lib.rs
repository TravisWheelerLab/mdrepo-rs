pub mod models;
pub mod ops;
pub mod schema;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::ConnectionError;

/// Establish a direct `PgConnection` (used by the CLI).
pub fn connect(url: &str) -> Result<PgConnection, ConnectionError> {
    PgConnection::establish(url)
}
