pub mod any_conn_wrapper;
pub mod connection;
pub mod interaction;
pub mod types;

pub use any_conn_wrapper::AnyConnWrapper;
pub use connection::MiddlewarePoolConnection;
pub use types::MiddlewarePool;

use crate::SqlMiddlewareDbError;
use crate::types::DatabaseType;

/// Configuration plus connection pool for a database backend.
///
/// Construct with the backend-specific `new_*` helpers on `ConfigAndPool` (e.g., `new_sqlite`,
/// `new_postgres`), then borrow connections as needed:
/// ```rust,no_run
/// use sql_middleware::prelude::*;
///
/// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
/// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
/// let mut conn = cap.get_connection().await?;
/// let rows = conn.query("SELECT 1").select().await?;
/// assert_eq!(rows.results.len(), 1);
/// # Ok(()) }
/// ```
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
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.execute_batch("CREATE TABLE t (id INTEGER)").await?;
    /// # Ok(()) }
    /// ```
    pub async fn get_connection(&self) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
        let pool_ref = self.pool.get().await?;
        MiddlewarePool::get_connection(pool_ref, self.translate_placeholders).await
    }
}
