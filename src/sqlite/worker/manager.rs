use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use deadpool::managed::ObjectId;
use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};

use super::channel::{BoxedCallback, Command};
use super::dispatcher::run_sqlite_worker;

pub(super) struct SqliteWorker {
    sender: Sender<Command>,
    object_id: ObjectId,
}

impl SqliteWorker {
    pub(super) fn spawn(object: Object) -> Result<Self, SqlMiddlewareDbError> {
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

    pub(super) fn object_id(&self) -> ObjectId {
        self.object_id
    }

    pub(super) fn send_command(&self, command: Command) -> Result<(), SqlMiddlewareDbError> {
        self.sender
            .send(command)
            .map_err(|_| connection_error("SQLite worker closed"))
    }

    pub(super) async fn request<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T, SqlMiddlewareDbError>>) -> Command,
        drop_message: &'static str,
    ) -> Result<T, SqlMiddlewareDbError> {
        let (tx, rx) = oneshot::channel();
        self.send_command(build(tx))?;
        rx.await.map_err(|_| connection_error(drop_message))?
    }

    pub(super) async fn execute_batch(&self, query: String) -> Result<(), SqlMiddlewareDbError> {
        self.request(
            |respond_to| Command::ExecuteBatch { query, respond_to },
            "SQLite worker dropped while executing batch",
        )
        .await
    }

    pub(super) async fn execute_select(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.request(
            |respond_to| Command::ExecuteSelect {
                query,
                params,
                respond_to,
            },
            "SQLite worker dropped while executing select",
        )
        .await
    }

    pub(super) async fn execute_dml(
        &self,
        query: String,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.request(
            |respond_to| Command::ExecuteDml {
                query,
                params,
                respond_to,
            },
            "SQLite worker dropped while executing dml",
        )
        .await
    }

    pub(super) async fn prepare_statement(
        &self,
        query: Arc<String>,
    ) -> Result<(), SqlMiddlewareDbError> {
        self.request(
            |respond_to| Command::PrepareStatement { query, respond_to },
            "SQLite worker dropped while preparing statement",
        )
        .await
    }

    pub(super) async fn execute_prepared_select(
        &self,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.request(
            |respond_to| Command::ExecutePreparedSelect {
                query,
                params,
                respond_to,
            },
            "SQLite worker dropped while executing prepared select",
        )
        .await
    }

    pub(super) async fn execute_prepared_dml(
        &self,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.request(
            |respond_to| Command::ExecutePreparedDml {
                query,
                params,
                respond_to,
            },
            "SQLite worker dropped while executing prepared dml",
        )
        .await
    }

    pub(super) async fn with_connection<F, R>(&self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let callback: BoxedCallback = Box::new(move |conn| {
            func(conn).map(|value| Box::new(value) as Box<dyn std::any::Any + Send>)
        });
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
            Err(_) => Err(connection_error(
                "SQLite worker dropped while handling custom callback",
            )),
        }
    }
}

impl Drop for SqliteWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
    }
}

fn connection_error(message: &str) -> SqlMiddlewareDbError {
    SqlMiddlewareDbError::ConnectionError(message.into())
}
