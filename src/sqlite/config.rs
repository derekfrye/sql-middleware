use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use bb8::{ManageConnection, Pool, PooledConnection};
use crossbeam_channel::{Sender, unbounded};

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Type alias for the pooled `SQLite` connection wrapper.
pub type SqlitePooledConnection = PooledConnection<'static, SqliteManager>;

/// Shared, worker-backed `SQLite` connection handle.
pub type SharedSqliteConnection = Arc<SqliteWorker>;

/// Test-only helper to rollback a connection from the pool.
#[doc(hidden)]
#[cfg(feature = "sqlite")]
pub async fn rollback_for_tests(pool: &Pool<SqliteManager>) -> Result<(), SqlMiddlewareDbError> {
    let conn = pool.get_owned().await.map_err(|e| {
        SqlMiddlewareDbError::ConnectionError(format!("sqlite cleanup checkout error: {e}"))
    })?;
    let handle = Arc::clone(&*conn);
    crate::sqlite::connection::run_blocking(handle, |c| {
        c.execute_batch("ROLLBACK;")
            .map_err(SqlMiddlewareDbError::SqliteError)
    })
    .await
}

enum SqliteWorkerMessage {
    Execute(Box<dyn FnOnce(&mut rusqlite::Connection) + Send + 'static>),
    Shutdown,
}

#[derive(Debug)]
pub struct SqliteWorker {
    sender: Sender<SqliteWorkerMessage>,
    broken: Arc<AtomicBool>,
}

impl SqliteWorker {
    pub(crate) fn start(conn: rusqlite::Connection) -> Arc<Self> {
        let (sender, receiver) = unbounded::<SqliteWorkerMessage>();
        let broken = Arc::new(AtomicBool::new(false));
        let broken_flag = Arc::clone(&broken);
        let mut conn = Some(conn);
        // Dedicated worker thread to service requests for this pooled connection.
        let _ = thread::Builder::new()
            .name("sql-middleware-sqlite-worker".into())
            .spawn(move || {
                let mut conn = conn
                    .take()
                    .expect("sqlite worker missing connection at start");
                for msg in &receiver {
                    match msg {
                        SqliteWorkerMessage::Execute(job) => {
                            // If a job panics, mark the worker broken and exit to avoid
                            // leaving the connection in an unknown state.
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    job(&mut conn);
                                }));
                            if result.is_err() {
                                broken_flag.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                        SqliteWorkerMessage::Shutdown => break,
                    }
                }
                broken_flag.store(true, Ordering::Relaxed);
            });

        Arc::new(Self { sender, broken })
    }

    pub(crate) fn execute<F>(&self, func: F) -> Result<(), SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) + Send + 'static,
    {
        self.sender
            .send(SqliteWorkerMessage::Execute(Box::new(func)))
            .map_err(|_| {
                SqlMiddlewareDbError::ExecutionError(
                    "sqlite worker channel unexpectedly closed".into(),
                )
            })
    }

    pub(crate) fn execute_blocking<F, R>(&self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        let (resp_tx, resp_rx) = crossbeam_channel::bounded(1);
        self.sender
            .send(SqliteWorkerMessage::Execute(Box::new(move |conn| {
                let _ = resp_tx.send(func(conn));
            })))
            .map_err(|_| {
                SqlMiddlewareDbError::ExecutionError(
                    "sqlite worker channel unexpectedly closed".into(),
                )
            })?;
        resp_rx.recv().map_err(|_| {
            SqlMiddlewareDbError::ExecutionError(
                "sqlite worker response channel unexpectedly closed".into(),
            )
        })?
    }

    #[must_use]
    pub(crate) fn is_broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    #[must_use]
    pub fn is_broken_for_tests(&self) -> bool {
        self.is_broken()
    }
}

impl Drop for SqliteWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(SqliteWorkerMessage::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bb8::Pool;

    use super::SqliteManager;
    use crate::middleware::SqlMiddlewareDbError;
    use crate::sqlite::connection::run_blocking;

    #[tokio::test]
    async fn worker_panic_marks_connection_broken() -> Result<(), Box<dyn std::error::Error>> {
        let pool = Pool::builder()
            .max_size(1)
            .build(SqliteManager::new("file::memory:?cache=shared".to_string()))
            .await?;

        let conn = pool.get_owned().await?;
        let handle = Arc::clone(&*conn);
        let err = run_blocking(handle, |_conn| -> Result<(), SqlMiddlewareDbError> {
            panic!("boom");
        })
        .await
        .expect_err("worker panic should surface as an error");
        assert!(
            err.to_string().contains("worker receive error"),
            "unexpected error for worker panic: {err}"
        );
        assert!(conn.is_broken(), "connection should be marked broken");

        drop(conn);

        let conn = pool.get_owned().await?;
        let handle = Arc::clone(&*conn);
        run_blocking(handle, |c| {
            c.query_row("SELECT 1", rusqlite::params![], |_row| Ok(()))
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        assert!(
            !conn.is_broken(),
            "replacement connection should be healthy"
        );
        Ok(())
    }
}

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

/// bb8 manager for `SQLite` connections.
pub struct SqliteManager {
    db_path: String,
}

impl SqliteManager {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }

    /// Build a pool from this manager.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if pool creation fails.
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
            Ok(SqliteWorker::start(conn))
        }
    }

    fn is_valid(
        &self,
        conn: &mut Self::Connection,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        let conn = Arc::clone(conn);
        async move {
            crate::sqlite::connection::run_blocking(conn, |guard| {
                guard
                    .query_row("SELECT 1", rusqlite::params![], |_row| Ok(()))
                    .map_err(SqlMiddlewareDbError::SqliteError)
            })
            .await
        }
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        conn.is_broken()
    }
}
