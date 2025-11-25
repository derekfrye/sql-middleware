use deadpool_tiberius::Manager as TiberiusManager;
use tiberius::Client;
use tokio::net::TcpStream;
use tokio_util::compat::Compat;

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Type alias for SQL Server client
pub type MssqlClient = Client<Compat<TcpStream>>;

/// Type alias for SQL Server Manager from deadpool-tiberius
pub type MssqlManager = TiberiusManager;

impl ConfigAndPool {
    /// Asynchronous initializer for `ConfigAndPool` with SQL Server (MSSQL)
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if MSSQL client creation or pool creation fails.
    #[allow(clippy::unused_async)]
    pub async fn new_mssql(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        instance_name: Option<String>, // For named instance support
    ) -> Result<Self, SqlMiddlewareDbError> {
        Self::new_mssql_with_translation(
            server,
            database,
            user,
            password,
            port,
            instance_name,
            false,
        )
        .await
    }

    /// Asynchronous initializer for `ConfigAndPool` with SQL Server (MSSQL) and optional translation default.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if MSSQL client creation or pool creation fails.
    #[allow(clippy::unused_async)]
    pub async fn new_mssql_with_translation(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        instance_name: Option<String>, // For named instance support
        translate_placeholders: bool,
    ) -> Result<Self, SqlMiddlewareDbError> {
        // Create deadpool-tiberius manager and configure it
        let mut manager = deadpool_tiberius::Manager::new()
            .host(&server)
            .port(port.unwrap_or(1433))
            .database(&database)
            .basic_authentication(&user, &password)
            .trust_cert();

        // Add instance name if provided
        if let Some(instance) = &instance_name {
            manager = manager.instance_name(instance);
        }

        // Create pool
        let pool = manager.max_size(20).create_pool().map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQL Server pool: {e}"))
        })?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Mssql(pool),
            db_type: DatabaseType::Mssql,
            translate_placeholders,
        })
    }
}
