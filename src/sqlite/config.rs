use deadpool_sqlite::{Config as DeadpoolSqliteConfig, Runtime};

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Options for configuring a `SQLite` pool.
#[derive(Debug, Clone)]
pub struct SqliteOptions {
    pub db_path: String,
    pub translate_placeholders: bool,
}

impl SqliteOptions {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            db_path,
            translate_placeholders: false,
        }
    }

    #[must_use]
    pub fn with_translation(mut self, translate_placeholders: bool) -> Self {
        self.translate_placeholders = translate_placeholders;
        self
    }
}

/// Fluent builder for `SQLite` options.
#[derive(Debug, Clone)]
pub struct SqliteOptionsBuilder {
    opts: SqliteOptions,
}

impl SqliteOptionsBuilder {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            opts: SqliteOptions::new(db_path),
        }
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn finish(self) -> SqliteOptions {
        self.opts
    }

    /// Build a `ConfigAndPool` for `SQLite`.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if pool creation or the initial smoke test fails.
    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_sqlite(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    pub fn sqlite_builder(db_path: String) -> SqliteOptionsBuilder {
        SqliteOptionsBuilder::new(db_path)
    }

    /// Asynchronous initializer for `ConfigAndPool` with Sqlite using `deadpool_sqlite`.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if pool creation or connection test fails.
    pub async fn new_sqlite(opts: SqliteOptions) -> Result<Self, SqlMiddlewareDbError> {
        let db_path = opts.db_path;
        let translate_placeholders = opts.translate_placeholders;

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
