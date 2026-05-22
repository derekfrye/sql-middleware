use std::path::PathBuf;

use crate::middleware::{
    ConfigAndPool, DatabaseType, MiddlewarePool, MiddlewarePoolOptions, SqlMiddlewareDbError,
};
use crate::turso::typed::TursoManager;
use crate::types::StatementCacheMode;

/// Options for configuring a Turso database.
#[derive(Debug, Clone)]
pub struct TursoOptions {
    pub db_path: PathBuf,
    pub translate_placeholders: bool,
    pub pool_options: MiddlewarePoolOptions,
    pub statement_cache_mode: StatementCacheMode,
}

impl TursoOptions {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            db_path: db_path.into(),
            translate_placeholders: false,
            pool_options: MiddlewarePoolOptions::default(),
            statement_cache_mode: StatementCacheMode::Cached,
        }
    }

    #[must_use]
    pub fn from_path(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            translate_placeholders: false,
            pool_options: MiddlewarePoolOptions::default(),
            statement_cache_mode: StatementCacheMode::Cached,
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
    pub fn with_test_on_check_out(mut self, test_on_check_out: bool) -> Self {
        self.pool_options.test_on_check_out = test_on_check_out;
        self
    }
}

/// Fluent builder for Turso options.
#[derive(Debug, Clone)]
pub struct TursoOptionsBuilder {
    opts: TursoOptions,
}

impl TursoOptionsBuilder {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            opts: TursoOptions::new(db_path),
        }
    }

    #[must_use]
    pub fn from_path(db_path: impl Into<PathBuf>) -> Self {
        Self {
            opts: TursoOptions::from_path(db_path),
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
    pub fn test_on_check_out(mut self, test_on_check_out: bool) -> Self {
        self.opts.pool_options.test_on_check_out = test_on_check_out;
        self
    }

    #[must_use]
    pub fn finish(self) -> TursoOptions {
        self.opts
    }

    /// Build a `ConfigAndPool` for Turso.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if database creation or connection testing fails.
    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_turso(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    pub fn turso_builder(db_path: String) -> TursoOptionsBuilder {
        TursoOptionsBuilder::new(db_path)
    }

    #[must_use]
    pub fn turso_path_builder(db_path: impl Into<PathBuf>) -> TursoOptionsBuilder {
        TursoOptionsBuilder::from_path(db_path)
    }

    /// Asynchronous initializer for `ConfigAndPool` with Turso (local/in-process).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if database creation or connection test fails.
    pub async fn new_turso(opts: TursoOptions) -> Result<Self, SqlMiddlewareDbError> {
        let db_path = opts.db_path;
        let translate_placeholders = opts.translate_placeholders;
        let pool_options = opts.pool_options;
        let statement_cache_mode = opts.statement_cache_mode;
        let db_path = db_path.to_str().ok_or_else(|| {
            SqlMiddlewareDbError::ConnectionError(
                "Turso local database paths must be valid UTF-8".into(),
            )
        })?;

        let db = turso::Builder::new_local(db_path)
            .build()
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!(
                    "Failed to create Turso database: {e}"
                ))
            })?;

        // Smoke-test a connection
        let conn = db.connect().map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to connect Turso database: {e}"))
        })?;

        // Best-effort pragmas for concurrency (ignore failure on in-memory/unsupported)
        let _ = conn.execute("PRAGMA journal_mode = WAL", ()).await;

        let manager = TursoManager::new(db).with_pool_options(pool_options);
        let pool = manager.build_pool().await?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Turso(pool),
            db_type: DatabaseType::Turso,
            translate_placeholders,
            statement_cache_mode,
        })
    }
}
