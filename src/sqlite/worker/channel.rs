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
    BeginTransaction {
        respond_to: oneshot::Sender<Result<u64, SqlMiddlewareDbError>>,
    },
    ExecuteTxBatch {
        tx_id: u64,
        query: String,
        respond_to: oneshot::Sender<Result<(), SqlMiddlewareDbError>>,
    },
    ExecuteTxQuery {
        tx_id: u64,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
        respond_to: oneshot::Sender<Result<ResultSet, SqlMiddlewareDbError>>,
    },
    ExecuteTxDml {
        tx_id: u64,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
        respond_to: oneshot::Sender<Result<usize, SqlMiddlewareDbError>>,
    },
    CommitTx {
        tx_id: u64,
        respond_to: oneshot::Sender<Result<(), SqlMiddlewareDbError>>,
    },
    RollbackTx {
        tx_id: u64,
        respond_to: oneshot::Sender<Result<(), SqlMiddlewareDbError>>,
    },
    WithConnection {
        callback: BoxedCallback,
        respond_to: oneshot::Sender<BoxedResponse>,
    },
    Shutdown,
}
