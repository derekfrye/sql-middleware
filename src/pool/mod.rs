pub mod types;
pub mod connection;
pub mod interaction;

pub use types::{MiddlewarePool, SqliteWritePool};
pub use connection::MiddlewarePoolConnection;

use crate::types::DatabaseType;

/// Configuration and connection pool for a database
///
/// This struct holds both the configuration and the connection pool
/// for a database, making it easier to manage database connections.
#[derive(Clone, Debug)]
pub struct ConfigAndPool {
    /// The connection pool
    pub pool: MiddlewarePool,
    /// The database type
    pub db_type: DatabaseType,
}