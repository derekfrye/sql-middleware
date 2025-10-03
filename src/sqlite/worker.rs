use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use deadpool::managed::ObjectId;
use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite::{self, ToSql};
use tokio::sync::oneshot;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};

use super::query::build_result_set;

/// Owned SQLite connection backed by a dedicated worker thread.
#[derive(Clone)]
pub struct SqliteConnection {
    worker: Arc<SqliteWorker>,
}

impl SqliteConnection {
    pub fn new(object: Object) -> Result<Self, SqlMiddlewareDbError> {
        let worker = SqliteWorker::spawn(object)?;
        Ok(Self {
            worker: Arc::new(worker),
        })
    }

    pub async fn execute_batch(&self, query: String) -> Result<(), SqlMiddlewareDbError> {
        self.worker.execute_batch(query).await
    }

    pub async fn execute_select(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.worker.execute_select(query, params).await
    }

    pub async fn execute_dml(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.worker.execute_dml(query, params).await
    }

    pub async fn with_connection<F, R>(&self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        self.worker.with_connection(func).await
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
        thread::Builder::new()
            .name(format!("sqlite-worker-{object_id}"))
            .spawn(move || run_sqlite_worker(&object, receiver))
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
    WithConnection {
        callback: BoxedCallback,
        respond_to: oneshot::Sender<BoxedResponse>,
    },
    Shutdown,
}

fn run_sqlite_worker(object: &Object, receiver: Receiver<Command>) {
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
