use deadpool_tiberius::Manager as TiberiusManager;
use tiberius::Client;
use tokio::net::TcpStream;
use tokio_util::compat::Compat;

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Type alias for SQL Server client
pub type MssqlClient = Client<Compat<TcpStream>>;

/// Type alias for SQL Server Manager from deadpool-tiberius
pub type MssqlManager = TiberiusManager;

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
    /// Returns `SqlMiddlewareDbError::ConnectionError` if MSSQL client creation or pool creation fails.
    #[allow(clippy::unused_async)]
    pub async fn new_mssql(opts: MssqlOptions) -> Result<Self, SqlMiddlewareDbError> {
        // Create deadpool-tiberius manager and configure it
        let mut manager = deadpool_tiberius::Manager::new()
            .host(&opts.server)
            .port(opts.port.unwrap_or(1433))
            .database(&opts.database)
            .basic_authentication(&opts.user, &opts.password)
            .trust_cert();

        // Add instance name if provided
        if let Some(instance) = &opts.instance_name {
            manager = manager.instance_name(instance);
        }

        // Create pool
        let pool = manager.max_size(20).create_pool().map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQL Server pool: {e}"))
        })?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Mssql(pool),
            db_type: DatabaseType::Mssql,
            translate_placeholders: opts.translate_placeholders,
        })
    }
}
