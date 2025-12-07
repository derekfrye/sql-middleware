use bb8::Pool;
use bb8_tiberius::{ConnectionManager, rt};
use tiberius::{AuthMethod, Config as TiberiusConfig};

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Type alias for SQL Server client
pub type MssqlClient = rt::Client;

/// Options for configuring an MSSQL pool.
#[derive(Debug, Clone)]
pub struct MssqlOptions {
    pub server: String,
    pub database: String,
    pub user: String,
    pub password: String,
    pub port: Option<u16>,
    pub instance_name: Option<String>,
    pub translate_placeholders: bool,
}

impl MssqlOptions {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        instance_name: Option<String>,
    ) -> Self {
        Self {
            server,
            database,
            user,
            password,
            port,
            instance_name,
            translate_placeholders: false,
        }
    }

    #[must_use]
    pub fn with_translation(mut self, translate_placeholders: bool) -> Self {
        self.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn with_port(mut self, port: Option<u16>) -> Self {
        self.port = port;
        self
    }

    #[must_use]
    pub fn with_instance_name(mut self, instance_name: Option<String>) -> Self {
        self.instance_name = instance_name;
        self
    }
}

/// Fluent builder for MSSQL options.
#[derive(Debug, Clone)]
pub struct MssqlOptionsBuilder {
    opts: MssqlOptions,
}

impl MssqlOptionsBuilder {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        instance_name: Option<String>,
    ) -> Self {
        Self {
            opts: MssqlOptions::new(server, database, user, password, port, instance_name),
        }
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn port(mut self, port: Option<u16>) -> Self {
        self.opts.port = port;
        self
    }

    #[must_use]
    pub fn instance_name(mut self, instance_name: Option<String>) -> Self {
        self.opts.instance_name = instance_name;
        self
    }

    #[must_use]
    pub fn finish(self) -> MssqlOptions {
        self.opts
    }

    /// Build a `ConfigAndPool` for SQL Server.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if pool creation fails.
    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_mssql(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn mssql_builder(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        instance_name: Option<String>,
    ) -> MssqlOptionsBuilder {
        MssqlOptionsBuilder::new(server, database, user, password, port, instance_name)
    }

    /// Asynchronous initializer for `ConfigAndPool` with SQL Server (MSSQL).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if MSSQL manager/pool creation fails.
    #[allow(clippy::unused_async)]
    pub async fn new_mssql(opts: MssqlOptions) -> Result<Self, SqlMiddlewareDbError> {
        let config = build_tiberius_config(&opts);

        let manager = ConnectionManager::build(config).map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!(
                "Failed to configure SQL Server manager: {e}"
            ))
        })?;

        let pool = Pool::builder()
            .max_size(20)
            .build(manager)
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!(
                    "Failed to create SQL Server pool: {e}"
                ))
            })?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Mssql(pool),
            db_type: DatabaseType::Mssql,
            translate_placeholders: opts.translate_placeholders,
        })
    }
}

fn build_tiberius_config(opts: &MssqlOptions) -> TiberiusConfig {
    let mut config = TiberiusConfig::new();
    config.host(&opts.server);
    config.database(&opts.database);
    config.port(opts.port.unwrap_or(1433));
    config.authentication(AuthMethod::sql_server(&opts.user, &opts.password));
    if let Some(instance) = &opts.instance_name {
        config.instance_name(instance);
    }
    config.trust_cert();
    config
}
