use deadpool_sqlite::{Config as DeadpoolSqliteConfig, Runtime};

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

impl ConfigAndPool {
    /// Asynchronous initializer for `ConfigAndPool` with Sqlite using `deadpool_sqlite`
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if pool creation or connection test fails.
    pub async fn new_sqlite(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        Self::new_sqlite_with_translation(db_path, false).await
    }

    /// Asynchronous initializer for `ConfigAndPool` with Sqlite using `deadpool_sqlite`
    /// and optional placeholder translation default.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if pool creation or connection test fails.
    pub async fn new_sqlite_with_translation(
        db_path: String,
        translate_placeholders: bool,
    ) -> Result<Self, SqlMiddlewareDbError> {
        // Configure deadpool_sqlite
        let cfg: DeadpoolSqliteConfig = DeadpoolSqliteConfig::new(db_path.clone());

        // Create the pool
        let pool = cfg.create_pool(Runtime::Tokio1).map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQLite pool: {e}"))
        })?;

        // Initialize the database (e.g., create tables)
        {
            let conn = pool
                .get()
                .await
                .map_err(SqlMiddlewareDbError::PoolErrorSqlite)?;
            let _res = conn
                .interact(|conn| {
                    conn.execute_batch(
                        "
                    PRAGMA journal_mode = WAL;
                ",
                    )
                    .map_err(SqlMiddlewareDbError::SqliteError)
                })
                .await?;
        }

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Sqlite(pool),
            db_type: DatabaseType::Sqlite,
            translate_placeholders,
        })
    }
}

/// Convert `InteractError` to a more specific `SqlMiddlewareDbError`
impl From<deadpool_sqlite::InteractError> for SqlMiddlewareDbError {
    fn from(err: deadpool_sqlite::InteractError) -> Self {
        SqlMiddlewareDbError::ConnectionError(format!("SQLite Interact Error: {err}"))
    }
}
