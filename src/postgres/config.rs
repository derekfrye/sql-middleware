use super::typed::PgManager;
use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Minimal Postgres configuration (keeps the public API backward-compatible
/// with the old `deadpool_postgres::Config` usage).
#[derive(Clone, Debug, Default)]
pub struct PgConfig {
    pub dbname: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
}

impl PgConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn to_tokio_config(&self) -> tokio_postgres::Config {
        let mut cfg = tokio_postgres::Config::new();
        if let Some(dbname) = &self.dbname {
            cfg.dbname(dbname);
        }
        if let Some(host) = &self.host {
            cfg.host(host);
        }
        if let Some(port) = self.port {
            cfg.port(port);
        }
        if let Some(user) = &self.user {
            cfg.user(user);
        }
        if let Some(password) = &self.password {
            cfg.password(password);
        }
        cfg
    }
}

/// Options for configuring a Postgres pool.
#[derive(Clone)]
pub struct PostgresOptions {
    pub config: PgConfig,
    pub translate_placeholders: bool,
}

impl PostgresOptions {
    #[must_use]
    pub fn new(config: PgConfig) -> Self {
        Self {
            config,
            translate_placeholders: false,
        }
    }

    #[must_use]
    pub fn with_translation(mut self, translate_placeholders: bool) -> Self {
        self.translate_placeholders = translate_placeholders;
        self
    }
}

/// Fluent builder for Postgres options.
#[derive(Clone)]
pub struct PostgresOptionsBuilder {
    opts: PostgresOptions,
}

impl PostgresOptionsBuilder {
    #[must_use]
    pub fn new(config: PgConfig) -> Self {
        Self {
            opts: PostgresOptions::new(config),
        }
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn finish(self) -> PostgresOptions {
        self.opts
    }

    /// Build a `ConfigAndPool` for `PostgreSQL`.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if pool creation fails.
    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_postgres(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    pub fn postgres_builder(pg_config: PgConfig) -> PostgresOptionsBuilder {
        PostgresOptionsBuilder::new(pg_config)
    }

    /// Asynchronous initializer for `ConfigAndPool` with Postgres.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConfigError` if required config fields are missing or `SqlMiddlewareDbError::ConnectionError` if pool creation fails.
    #[allow(clippy::unused_async)]
    pub async fn new_postgres(opts: PostgresOptions) -> Result<Self, SqlMiddlewareDbError> {
        let pg_config = opts.config;
        let translate_placeholders = opts.translate_placeholders;

        // Validate all required config fields are present
        if pg_config.dbname.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError(
                "dbname is required".to_string(),
            ));
        }

        if pg_config.host.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError(
                "host is required".to_string(),
            ));
        }
        if pg_config.port.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError(
                "port is required".to_string(),
            ));
        }
        if pg_config.user.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError(
                "user is required".to_string(),
            ));
        }
        if pg_config.password.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError(
                "password is required".to_string(),
            ));
        }

        // Attempt to create connection pool
        let manager = PgManager::new(pg_config.to_tokio_config());
        let pg_pool = manager.build_pool().await?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Postgres(pg_pool),
            db_type: DatabaseType::Postgres,
            translate_placeholders,
        })
    }
}
