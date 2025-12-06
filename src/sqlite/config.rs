use std::sync::Arc;

use bb8::{ManageConnection, Pool, PooledConnection};
use tokio::task::spawn_blocking;

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Type alias for the pooled SQLite connection wrapper.
pub type SqlitePooledConnection = PooledConnection<'static, SqliteManager>;

/// Shared, mutex-guarded SQLite connection handle.
pub type SharedSqliteConnection = Arc<tokio::sync::Mutex<rusqlite::Connection>>;

/// Options for configuring a `SQLite` pool.
#[derive(Debug, Clone)]
pub struct SqliteOptions {
    pub db_path: String,
    pub translate_placeholders: bool,
}

impl SqliteOptions {
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

/// Fluent builder for `SQLite` options.
#[derive(Debug, Clone)]
pub struct SqliteOptionsBuilder {
    opts: SqliteOptions,
}

impl SqliteOptionsBuilder {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            opts: SqliteOptions::new(db_path),
        }
    }

    #[must_use]
    pub fn translation(mut self, translate_placeholders: bool) -> Self {
        self.opts.translate_placeholders = translate_placeholders;
        self
    }

    #[must_use]
    pub fn finish(self) -> SqliteOptions {
        self.opts
    }

    /// Build a `ConfigAndPool` for `SQLite`.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if pool creation or the initial smoke test fails.
    pub async fn build(self) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
        ConfigAndPool::new_sqlite(self.finish()).await
    }
}

impl ConfigAndPool {
    #[must_use]
    pub fn sqlite_builder(db_path: String) -> SqliteOptionsBuilder {
        SqliteOptionsBuilder::new(db_path)
    }

    /// Asynchronous initializer for `ConfigAndPool` with Sqlite using a bb8-backed pool.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if pool creation or connection test fails.
    pub async fn new_sqlite(opts: SqliteOptions) -> Result<Self, SqlMiddlewareDbError> {
        let manager = SqliteManager::new(opts.db_path.clone());
        let pool = manager.build_pool().await?;

        // Initialize the database with WAL and a simple health check.
        {
            let mut conn = pool.get_owned().await.map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQLite pool: {e}"))
            })?;

            crate::sqlite::apply_wal_pragmas(&mut conn).await?;
        }

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Sqlite(pool),
            db_type: DatabaseType::Sqlite,
            translate_placeholders: opts.translate_placeholders,
        })
    }
}

/// bb8 manager for SQLite connections.
pub struct SqliteManager {
    db_path: String,
}

impl SqliteManager {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }

    /// Build a pool from this manager.
    pub async fn build_pool(self) -> Result<Pool<SqliteManager>, SqlMiddlewareDbError> {
        Pool::builder()
            .build(self)
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("sqlite pool error: {e}")))
    }
}

impl ManageConnection for SqliteManager {
    type Connection = SharedSqliteConnection;
    type Error = SqlMiddlewareDbError;

    fn connect(
        &self,
    ) -> impl std::future::Future<Output = Result<Self::Connection, Self::Error>> + Send {
        let path = self.db_path.clone();
        async move {
            let conn =
                rusqlite::Connection::open(path).map_err(SqlMiddlewareDbError::SqliteError)?;
            Ok(Arc::new(tokio::sync::Mutex::new(conn)))
        }
    }

    fn is_valid(
        &self,
        conn: &mut Self::Connection,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        let conn = Arc::clone(conn);
        async move {
            spawn_blocking(move || {
                let guard = conn.blocking_lock();
                guard
                    .query_row("SELECT 1", rusqlite::params![], |_row| Ok(()))
                    .map_err(SqlMiddlewareDbError::SqliteError)
            })
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("sqlite validation join error: {e}"))
            })?
        }
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}
