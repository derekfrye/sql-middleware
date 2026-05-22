use std::path::PathBuf;

use crate::middleware::{
    ConfigAndPool, DatabaseType, MiddlewarePool, MiddlewarePoolOptions, SqlMiddlewareDbError,
};
use crate::types::StatementCacheMode;

use super::config::SqliteManager;

/// Options for configuring a `SQLite` pool.
#[derive(Debug, Clone)]
pub struct SqliteOptions {
    pub db_path: PathBuf,
    pub translate_placeholders: bool,
    pub pool_options: MiddlewarePoolOptions,
    pub statement_cache_mode: StatementCacheMode,
    pub statement_cache_capacity: Option<usize>,
}

impl SqliteOptions {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            db_path: db_path.into(),
            translate_placeholders: false,
            pool_options: MiddlewarePoolOptions::default(),
            statement_cache_mode: StatementCacheMode::Cached,
            statement_cache_capacity: None,
        }
    }

    #[must_use]
    pub fn from_path(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            translate_placeholders: false,
            pool_options: MiddlewarePoolOptions::default(),
            statement_cache_mode: StatementCacheMode::Cached,
            statement_cache_capacity: None,
        }
    }

    #[must_use]
    pub fn with_translation(mut self, translate_placeholders: bool) -> Self {
        self.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn with_pool_options(mut self, pool_options: MiddlewarePoolOptions) -> Self {
        self.pool_options = pool_options;
        self
    }

    #[must_use]
    pub fn with_statement_cache(mut self, statement_cache_mode: StatementCacheMode) -> Self {
        self.statement_cache_mode = statement_cache_mode;
        self
    }

    #[must_use]
    pub fn with_statement_cache_capacity(mut self, capacity: usize) -> Self {
        self.statement_cache_capacity = Some(capacity);
        self
    }

    #[must_use]
    pub fn with_test_on_check_out(mut self, test_on_check_out: bool) -> Self {
        self.pool_options.test_on_check_out = test_on_check_out;
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
    pub fn from_path(db_path: impl Into<PathBuf>) -> Self {
        Self {
            opts: SqliteOptions::from_path(db_path),
        }
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn pool_options(mut self, pool_options: MiddlewarePoolOptions) -> Self {
        self.opts.pool_options = pool_options;
        self
    }

    #[must_use]
    pub fn statement_cache(mut self, statement_cache_mode: StatementCacheMode) -> Self {
        self.opts.statement_cache_mode = statement_cache_mode;
        self
    }

    #[must_use]
    pub fn statement_cache_capacity(mut self, capacity: usize) -> Self {
        self.opts.statement_cache_capacity = Some(capacity);
        self
    }

    #[must_use]
    pub fn test_on_check_out(mut self, test_on_check_out: bool) -> Self {
        self.opts.pool_options.test_on_check_out = test_on_check_out;
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

    #[must_use]
    pub fn sqlite_path_builder(db_path: impl Into<PathBuf>) -> SqliteOptionsBuilder {
        SqliteOptionsBuilder::from_path(db_path)
    }

    /// Asynchronous initializer for `ConfigAndPool` with Sqlite using a bb8-backed pool.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if pool creation or connection test fails.
    pub async fn new_sqlite(opts: SqliteOptions) -> Result<Self, SqlMiddlewareDbError> {
        let pool_options = opts.pool_options;
        let manager = SqliteManager::from_path(opts.db_path.clone())
            .with_pool_options(pool_options)
            .with_statement_cache_capacity(opts.statement_cache_capacity);
        let pool = manager.build_pool().await?;

        {
            let mut conn = pool.get_owned().await.map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQLite pool: {e}"))
            })?;

            crate::sqlite::apply_wal_pragmas(&mut conn).await?;
        }

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Sqlite(pool),
            db_type: DatabaseType::Sqlite,
            translate_placeholders: opts.translate_placeholders,
            statement_cache_mode: opts.statement_cache_mode,
        })
    }
}
