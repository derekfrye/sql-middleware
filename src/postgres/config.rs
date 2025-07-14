use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};
use deadpool_postgres::{Config as PgConfig};
use tokio_postgres::NoTls;

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Postgres
    pub async fn new_postgres(pg_config: PgConfig) -> Result<Self, SqlMiddlewareDbError> {
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
        let pg_pool = pg_config
            .create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls)
            .map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!(
                    "Failed to create Postgres pool: {e}"
                ))
            })?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Postgres(pg_pool),
            db_type: DatabaseType::Postgres,
        })
    }
}