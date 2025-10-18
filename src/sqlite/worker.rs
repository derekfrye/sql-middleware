use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use deadpool::managed::ObjectId;
use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite::{self, ToSql};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};

use super::prepared::SqlitePreparedStatement;
use super::query::build_result_set;

/// Owned `SQLite` connection backed by a dedicated worker thread.
#[derive(Clone)]
pub struct SqliteConnection {
    worker: Arc<SqliteWorker>,
}

impl SqliteConnection {
    /// Construct a worker-backed `SQLite` connection handle.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if the background worker thread cannot be spawned.
    pub fn new(object: Object) -> Result<Self, SqlMiddlewareDbError> {
        let worker = SqliteWorker::spawn(object)?;
        Ok(Self {
            worker: Arc::new(worker),
        })
    }

    /// Execute a batch of SQL statements on the worker-owned connection.
    ///
    /// # Errors
    /// Propagates any [`SqlMiddlewareDbError`] produced while dispatching the command or running
    /// the query batch within the worker.
    pub async fn execute_batch(&self, query: String) -> Result<(), SqlMiddlewareDbError> {
        self.worker.execute_batch(query).await
    }

    /// Execute a SQL query and return a [`ResultSet`] produced by the worker thread.
    ///
    /// # Errors
    /// Returns any [`SqlMiddlewareDbError`] encountered while the worker prepares or evaluates the
    /// statement, or if channel communication with the worker fails.
    pub async fn execute_select(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.worker.execute_select(query, params).await
    }

    /// Execute a DML statement (INSERT/UPDATE/DELETE) and return the affected row count.
    ///
    /// # Errors
    /// Returns any [`SqlMiddlewareDbError`] reported by the worker while executing the statement or
    /// relaying the result back to the caller.
    pub async fn execute_dml(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.worker.execute_dml(query, params).await
    }

    /// Run synchronous `rusqlite` logic against the underlying worker-owned connection.
    ///
    /// # Errors
    /// Propagates any [`SqlMiddlewareDbError`] raised while executing the callback or interacting
    /// with the worker.
    pub async fn with_connection<F, R>(&self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        self.worker.with_connection(func).await
    }

    /// Prepare a cached statement on the worker and return a reusable handle.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if the worker fails to prepare the statement or if the
    /// preparation channel is unexpectedly closed.
    pub async fn prepare_statement(
        &self,
        query: &str,
    ) -> Result<SqlitePreparedStatement, SqlMiddlewareDbError> {
        let query_arc = Arc::new(query.to_owned());
        self.worker
            .prepare_statement(Arc::clone(&query_arc))
            .await?;
        Ok(SqlitePreparedStatement::new(self.clone(), query_arc))
    }

    /// Execute a previously prepared query statement on the worker.
    ///
    /// # Errors
    /// Returns any [`SqlMiddlewareDbError`] produced while dispatching the command or running the
    /// query on the worker connection.
    pub(crate) async fn execute_prepared_select(
        &self,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.worker.execute_prepared_select(query, params).await
    }

    /// Execute a previously prepared DML statement on the worker.
    ///
    /// # Errors
    /// Returns any [`SqlMiddlewareDbError`] encountered while the worker runs the statement or if
    /// communication with the worker fails.
    pub(crate) async fn execute_prepared_dml(
        &self,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.worker.execute_prepared_dml(query, params).await
    }

    fn object_id(&self) -> ObjectId {
        self.worker.object_id
    }
}

impl fmt::Debug for SqliteConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteConnection")
            .field("object_id", &self.object_id())
            .finish()
    }
}

struct SqliteWorker {
    sender: Sender<Command>,
    object_id: ObjectId,
}

impl SqliteWorker {
    fn spawn(object: Object) -> Result<Self, SqlMiddlewareDbError> {
        let (sender, receiver) = mpsc::channel::<Command>();
        let object_id = Object::id(&object);
        let handle = Handle::try_current().ok();
        thread::Builder::new()
            .name(format!("sqlite-worker-{object_id}"))
            .spawn(move || {
                let runtime_guard = handle.as_ref().map(|h| h.enter());
                run_sqlite_worker(&object, &receiver);
                drop(runtime_guard);
            })
            .map_err(|err| {
                SqlMiddlewareDbError::ConnectionError(format!(
                    "failed to spawn SQLite worker thread: {err}"
                ))
            })?;

        Ok(Self { sender, object_id })
    }

    fn send_command(&self, command: Command) -> Result<(), SqlMiddlewareDbError> {
        self.sender
            .send(command)
            .map_err(|_| SqlMiddlewareDbError::ConnectionError("SQLite worker closed".into()))
    }

    async fn execute_batch(&self, query: String) -> Result<(), SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::ExecuteBatch {
            query,
            respond_to: tx,
        })?;
        rx.await.map_err(|_| {
            SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while executing batch".into(),
            )
        })?
    }

    async fn execute_select(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::ExecuteSelect {
            query,
            params,
            respond_to: tx,
        })?;
        rx.await.map_err(|_| {
            SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while executing select".into(),
            )
        })?
    }

    async fn execute_dml(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::ExecuteDml {
            query,
            params,
            respond_to: tx,
        })?;
        rx.await.map_err(|_| {
            SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while executing dml".into(),
            )
        })?
    }

    async fn prepare_statement(&self, query: Arc<String>) -> Result<(), SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::PrepareStatement {
            query,
            respond_to: tx,
        })?;
        rx.await.map_err(|_| {
            SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while preparing statement".into(),
            )
        })?
    }

    async fn execute_prepared_select(
        &self,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::ExecutePreparedSelect {
            query,
            params,
            respond_to: tx,
        })?;
        rx.await.map_err(|_| {
            SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while executing prepared select".into(),
            )
        })?
    }

    async fn execute_prepared_dml(
        &self,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(Command::ExecutePreparedDml {
            query,
            params,
            respond_to: tx,
        })?;
        rx.await.map_err(|_| {
            SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while executing prepared dml".into(),
            )
        })?
    }

    async fn with_connection<F, R>(&self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let callback: BoxedCallback =
            Box::new(move |conn| func(conn).map(|value| Box::new(value) as Box<dyn Any + Send>));
        self.send_command(Command::WithConnection {
            callback,
            respond_to: tx,
        })?;
        match rx.await {
            Ok(Ok(payload)) => payload.downcast::<R>().map(|boxed| *boxed).map_err(|_| {
                SqlMiddlewareDbError::ExecutionError(
                    "SQLite worker response downcast failure".into(),
                )
            }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(SqlMiddlewareDbError::ConnectionError(
                "SQLite worker dropped while handling custom callback".into(),
            )),
        }
    }
}

impl Drop for SqliteWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
    }
}

type BoxedResponse = Result<Box<dyn Any + Send>, SqlMiddlewareDbError>;
type BoxedCallback = Box<dyn FnOnce(&mut rusqlite::Connection) -> BoxedResponse + Send>;

enum Command {
    ExecuteBatch {
        query: String,
        respond_to: oneshot::Sender<Result<(), SqlMiddlewareDbError>>,
    },
    ExecuteSelect {
        query: String,
        params: Vec<rusqlite::types::Value>,
        respond_to: oneshot::Sender<Result<ResultSet, SqlMiddlewareDbError>>,
    },
    ExecuteDml {
        query: String,
        params: Vec<rusqlite::types::Value>,
        respond_to: oneshot::Sender<Result<usize, SqlMiddlewareDbError>>,
    },
    PrepareStatement {
        query: Arc<String>,
        respond_to: oneshot::Sender<Result<(), SqlMiddlewareDbError>>,
    },
    ExecutePreparedSelect {
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
        respond_to: oneshot::Sender<Result<ResultSet, SqlMiddlewareDbError>>,
    },
    ExecutePreparedDml {
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
        respond_to: oneshot::Sender<Result<usize, SqlMiddlewareDbError>>,
    },
    WithConnection {
        callback: BoxedCallback,
        respond_to: oneshot::Sender<BoxedResponse>,
    },
    Shutdown,
}

fn run_sqlite_worker(object: &Object, receiver: &Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::ExecuteBatch { query, respond_to } => {
                let outcome = with_connection(object, |conn| -> Result<_, SqlMiddlewareDbError> {
                    let tx = conn.transaction()?;
                    tx.execute_batch(&query)?;
                    tx.commit()?;
                    Ok(())
                });
                let _ = respond_to.send(outcome);
            }
            Command::ExecuteSelect {
                query,
                params,
                respond_to,
            } => {
                let outcome = with_connection(object, |conn| -> Result<_, SqlMiddlewareDbError> {
                    let mut stmt = conn.prepare(&query)?;
                    build_result_set(&mut stmt, &params)
                });
                let _ = respond_to.send(outcome);
            }
            Command::ExecuteDml {
                query,
                params,
                respond_to,
            } => {
                let outcome =
                    with_connection(object, |conn| -> Result<usize, SqlMiddlewareDbError> {
                        let tx = conn.transaction()?;
                        let param_refs: Vec<&dyn ToSql> =
                            params.iter().map(|value| value as &dyn ToSql).collect();
                        let rows_affected = {
                            let mut stmt = tx.prepare(&query)?;
                            stmt.execute(&param_refs[..])?
                        };
                        tx.commit()?;
                        Ok(rows_affected)
                    });
                let _ = respond_to.send(outcome);
            }
            Command::PrepareStatement { query, respond_to } => {
                let outcome = with_connection(object, |conn| -> Result<_, SqlMiddlewareDbError> {
                    let _ = conn.prepare_cached(&query)?;
                    Ok(())
                });
                let _ = respond_to.send(outcome);
            }
            Command::ExecutePreparedSelect {
                query,
                params,
                respond_to,
            } => {
                let outcome = with_connection(object, |conn| -> Result<_, SqlMiddlewareDbError> {
                    let mut stmt = conn.prepare_cached(&query)?;
                    build_result_set(&mut stmt, &params)
                });
                let _ = respond_to.send(outcome);
            }
            Command::ExecutePreparedDml {
                query,
                params,
                respond_to,
            } => {
                let outcome =
                    with_connection(object, |conn| -> Result<usize, SqlMiddlewareDbError> {
                        let tx = conn.transaction()?;
                        let param_refs: Vec<&dyn ToSql> =
                            params.iter().map(|value| value as &dyn ToSql).collect();
                        let rows_affected = {
                            let mut stmt = tx.prepare_cached(&query)?;
                            stmt.execute(&param_refs[..])?
                        };
                        tx.commit()?;
                        Ok(rows_affected)
                    });
                let _ = respond_to.send(outcome);
            }
            Command::WithConnection {
                callback,
                respond_to,
            } => {
                let outcome = with_connection(object, callback);
                let _ = respond_to.send(outcome);
            }
            Command::Shutdown => break,
        }
    }
}

fn with_connection<T, F>(object: &Object, f: F) -> Result<T, SqlMiddlewareDbError>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<T, SqlMiddlewareDbError>,
{
    let mut guard = object.lock().map_err(|err| {
        SqlMiddlewareDbError::ConnectionError(format!("SQLite connection mutex poisoned: {err}"))
    })?;
    f(&mut guard)
}
