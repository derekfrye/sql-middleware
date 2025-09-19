use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

impl ConfigAndPool {
    /// Asynchronous initializer for `ConfigAndPool` with Turso (local/in-process).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if database creation or connection test fails.
    pub async fn new_turso(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        let db = turso::Builder::new_local(&db_path)
            .build()
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to create Turso database: {e}")))?;

        // Smoke-test a connection
        let conn = db
            .connect()
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to connect Turso database: {e}")))?;

        // Best-effort pragmas for concurrency (ignore failure on in-memory/unsupported)
        let _ = conn.execute("PRAGMA journal_mode = WAL", ()).await;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Turso(db),
            db_type: DatabaseType::Turso,
        })
    }
}

