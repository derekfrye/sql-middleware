use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use bb8::{ManageConnection, Pool, PooledConnection};
use crossbeam_channel::{Sender, unbounded};

use crate::middleware::SqlMiddlewareDbError;

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
    force_rollback_busy_for_tests: AtomicBool,
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

        Arc::new(Self {
            sender,
            broken,
            force_rollback_busy_for_tests: AtomicBool::new(false),
        })
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

    pub(crate) fn mark_broken(&self) {
        self.broken.store(true, Ordering::Relaxed);
    }

    #[doc(hidden)]
    pub fn set_force_rollback_busy_for_tests(&self, force: bool) {
        self.force_rollback_busy_for_tests
            .store(force, Ordering::Relaxed);
    }

    pub(crate) fn force_rollback_busy_for_tests(&self) -> bool {
        self.force_rollback_busy_for_tests.load(Ordering::Relaxed)
    }
}

impl Drop for SqliteWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(SqliteWorkerMessage::Shutdown);
    }
}

/// bb8 manager for `SQLite` connections.
pub struct SqliteManager {
    db_path: PathBuf,
}

impl SqliteManager {
    #[must_use]
    pub fn new(db_path: String) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    #[must_use]
    pub fn from_path(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
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
