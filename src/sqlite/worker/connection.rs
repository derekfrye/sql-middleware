use std::fmt;
use std::sync::Arc;

use deadpool::managed::ObjectId;
use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};
use crate::sqlite::prepared::SqlitePreparedStatement;

use super::manager::SqliteWorker;

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
        self.worker.object_id()
    }

    pub(crate) async fn begin_transaction(&self) -> Result<u64, SqlMiddlewareDbError> {
        self.worker.begin_transaction().await
    }

    pub(crate) async fn execute_tx_batch(
        &self,
        tx_id: u64,
        query: String,
    ) -> Result<(), SqlMiddlewareDbError> {
        self.worker.execute_tx_batch(tx_id, query).await
    }

    pub(crate) async fn execute_tx_query(
        &self,
        tx_id: u64,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.worker.execute_tx_query(tx_id, query, params).await
    }

    pub(crate) async fn execute_tx_dml(
        &self,
        tx_id: u64,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.worker.execute_tx_dml(tx_id, query, params).await
    }

    #[doc(hidden)]
    pub async fn test_execute_tx_dml(
        &self,
        tx_id: u64,
        query: Arc<String>,
        params: Vec<rusqlite::types::Value>,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.execute_tx_dml(tx_id, query, params).await
    }

    pub(crate) async fn commit_tx(&self, tx_id: u64) -> Result<(), SqlMiddlewareDbError> {
        self.worker.commit_tx(tx_id).await
    }

    pub(crate) async fn rollback_tx(&self, tx_id: u64) -> Result<(), SqlMiddlewareDbError> {
        self.worker.rollback_tx(tx_id).await
    }
}

impl fmt::Debug for SqliteConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteConnection")
            .field("object_id", &self.object_id())
            .finish()
    }
}
