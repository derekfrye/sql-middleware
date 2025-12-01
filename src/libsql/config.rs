use deadpool_libsql::{Manager, Pool};

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Options for configuring a local LibSQL database.
#[derive(Debug, Clone)]
pub struct LibsqlOptions {
    pub db_path: String,
    pub translate_placeholders: bool,
}

impl LibsqlOptions {
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

/// Options for configuring a remote LibSQL/Turso database.
#[derive(Debug, Clone)]
pub struct LibsqlRemoteOptions {
    pub url: String,
    pub auth_token: Option<String>,
    pub translate_placeholders: bool,
}

impl LibsqlRemoteOptions {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self {
            url,
            auth_token: None,
            translate_placeholders: false,
        }
    }

    #[must_use]
    pub fn with_auth_token(mut self, auth_token: Option<String>) -> Self {
        self.auth_token = auth_token;
        self
    }

    #[must_use]
    pub fn with_translation(mut self, translate_placeholders: bool) -> Self {
        self.translate_placeholders = translate_placeholders;
        self
    }
}

/// Fluent builder for local LibSQL options.
#[derive(Debug, Clone)]
pub struct LibsqlOptionsBuilder {
    opts: LibsqlOptions,
}

impl LibsqlOptionsBuilder {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            opts: LibsqlOptions::new(db_path),
        }
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn finish(self) -> LibsqlOptions {
        self.opts
    }

    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_libsql(self.finish()).await
    }
}

/// Fluent builder for remote LibSQL options.
#[derive(Debug, Clone)]
pub struct LibsqlRemoteOptionsBuilder {
    opts: LibsqlRemoteOptions,
}

impl LibsqlRemoteOptionsBuilder {
    #[must_use]
    pub fn new(url: String) -> Self {
        Self {
            opts: LibsqlRemoteOptions::new(url),
        }
    }

    #[must_use]
    pub fn auth_token(mut self, auth_token: Option<String>) -> Self {
        self.opts.auth_token = auth_token;
        self
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn finish(self) -> LibsqlRemoteOptions {
        self.opts
    }

    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_libsql_remote(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    pub fn libsql_builder(db_path: String) -> LibsqlOptionsBuilder {
        LibsqlOptionsBuilder::new(db_path)
    }

    #[must_use]
    pub fn libsql_remote_builder(url: String) -> LibsqlRemoteOptionsBuilder {
        LibsqlRemoteOptionsBuilder::new(url)
    }

    /// Asynchronous initializer for `ConfigAndPool` with libsql using `deadpool_libsql`.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if database creation, pool creation, or connection test fails.
    pub async fn new_libsql(opts: LibsqlOptions) -> Result<Self, SqlMiddlewareDbError> {
        let translate_placeholders = opts.translate_placeholders;

        // Create libsql database connection
        let db = deadpool_libsql::libsql::Builder::new_local(opts.db_path.clone())
            .build()
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!(
                    "Failed to create libsql database: {e}"
                ))
            })?;

        // Create the manager
        let manager = Manager::from_libsql_database(db);

        // Create the pool
        let pool = Pool::builder(manager).build().map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create libsql pool: {e}"))
        })?;

        // Test the connection
        let conn = pool.get().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to get libsql connection: {e}"))
        })?;

        // Initialize the database with WAL mode for better concurrency (ignore result for in-memory databases)
        let _ = conn.execute("PRAGMA journal_mode = WAL", ()).await;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Libsql(pool),
            db_type: DatabaseType::Libsql,
            translate_placeholders,
        })
    }

    /// Create libsql connection from remote URL (Turso).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if remote database creation, pool creation, or connection test fails.
    pub async fn new_libsql_remote(
        opts: LibsqlRemoteOptions,
    ) -> Result<Self, SqlMiddlewareDbError> {
        let translate_placeholders = opts.translate_placeholders;

        // Create libsql database connection for remote
        let builder = deadpool_libsql::libsql::Builder::new_remote(
            opts.url.clone(),
            opts.auth_token.unwrap_or_default(),
        );

        let db = builder.build().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!(
                "Failed to create remote libsql database: {e}"
            ))
        })?;

        // Create the manager
        let manager = Manager::from_libsql_database(db);

        // Create the pool
        let pool = Pool::builder(manager).build().map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create libsql pool: {e}"))
        })?;

        // Test the connection
        let _conn = pool.get().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to get libsql connection: {e}"))
        })?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Libsql(pool),
            db_type: DatabaseType::Libsql,
            translate_placeholders,
        })
    }
}
