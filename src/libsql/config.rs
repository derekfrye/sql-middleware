use deadpool_libsql::{Manager, Pool};

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

impl ConfigAndPool {
    /// Asynchronous initializer for `ConfigAndPool` with libsql using `deadpool_libsql`
    pub async fn new_libsql(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        // Create libsql database connection
        let db = deadpool_libsql::libsql::Builder::new_local(db_path.clone())
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
        })
    }

    /// Create libsql connection from remote URL (Turso)
    pub async fn new_libsql_remote(
        url: String,
        auth_token: Option<String>,
    ) -> Result<Self, SqlMiddlewareDbError> {
        // Create libsql database connection for remote
        let builder = deadpool_libsql::libsql::Builder::new_remote(
            url.clone(),
            auth_token.unwrap_or_default(),
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
        })
    }
}
