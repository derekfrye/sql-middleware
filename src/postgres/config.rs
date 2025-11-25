use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};
use deadpool_postgres::Config as PgConfig;
use tokio_postgres::NoTls;

impl ConfigAndPool {
    /// Asynchronous initializer for `ConfigAndPool` with Postgres
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConfigError` if required config fields are missing or `SqlMiddlewareDbError::ConnectionError` if pool creation fails.
    #[allow(clippy::unused_async)]
    pub async fn new_postgres(pg_config: PgConfig) -> Result<Self, SqlMiddlewareDbError> {
        Self::new_postgres_with_translation(pg_config, false).await
    }

    /// Asynchronous initializer for `ConfigAndPool` with Postgres and optional translation default.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConfigError` if required config fields are missing or `SqlMiddlewareDbError::ConnectionError` if pool creation fails.
    ///
    /// Warning: translation skips placeholders inside quoted strings, comments, and dollar-quoted
    /// blocks via a lightweight state machine; it may miss edge cases in complex SQL (e.g.,
    /// PL/pgSQL bodies). Prefer backend-specific SQL instead of relying on translation:
    /// ```rust
    /// # use sql_middleware::prelude::*;
    /// let query = match &conn {
    ///     MiddlewarePoolConnection::Postgres { .. } => r#"$function$
    /// BEGIN
    ///     RETURN ($1 ~ $q$[\t\r\n\v\\]$q$);
    /// END;
    /// $function$"#,
    ///     MiddlewarePoolConnection::Sqlite { .. } | MiddlewarePoolConnection::Turso { .. } => {
    ///         include_str!("../sql/functions/sqlite/03_sp_get_scores.sql")
    ///     }
    /// };
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn new_postgres_with_translation(
        pg_config: PgConfig,
        translate_placeholders: bool,
    ) -> Result<Self, SqlMiddlewareDbError> {
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
            translate_placeholders,
        })
    }
}
