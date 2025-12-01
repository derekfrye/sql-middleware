use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Options for configuring a Turso database.
#[derive(Debug, Clone)]
pub struct TursoOptions {
    pub db_path: String,
    pub translate_placeholders: bool,
}

impl TursoOptions {
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
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn finish(self) -> TursoOptions {
        self.opts
    }

    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_turso(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    pub fn turso_builder(db_path: String) -> TursoOptionsBuilder {
        TursoOptionsBuilder::new(db_path)
    }

    /// Asynchronous initializer for `ConfigAndPool` with Turso (local/in-process).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if database creation or connection test fails.
    pub async fn new_turso(opts: TursoOptions) -> Result<Self, SqlMiddlewareDbError> {
        let db_path = opts.db_path;
        let translate_placeholders = opts.translate_placeholders;

        let db = turso::Builder::new_local(&db_path)
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

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Turso(db),
            db_type: DatabaseType::Turso,
            translate_placeholders,
        })
    }
}
