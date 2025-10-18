use std::any::Any;
use std::sync::Arc;

use deadpool_sqlite::rusqlite;
use tokio::sync::oneshot;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};

pub(super) type BoxedResponse = Result<Box<dyn Any + Send>, SqlMiddlewareDbError>;
pub(super) type BoxedCallback = Box<dyn FnOnce(&mut rusqlite::Connection) -> BoxedResponse + Send>;

pub(super) enum Command {
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
