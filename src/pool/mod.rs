pub mod connection;
pub mod interaction;
pub mod types;

pub use connection::MiddlewarePoolConnection;
pub use types::{MiddlewarePool, SqliteWritePool};

use crate::SqlMiddlewareDbError;
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
    /// Whether placeholder translation is enabled by default for this pool
    pub translate_placeholders: bool,
}

impl ConfigAndPool {
    /// Get a pooled connection and attach pool-level defaults to it.
    ///
    /// # Errors
    /// Bubbles up pool checkout errors for the active backend.
    pub async fn get_connection(&self) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
        let pool_ref = self.pool.get().await?;
        MiddlewarePool::get_connection(pool_ref, self.translate_placeholders).await
    }
}
