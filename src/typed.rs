//! Experimental typestate API (SQLite-only prototype).
//!
//! Provides `Connection<Idle>` / `Connection<InTx>` backed by a bb8 pool of
//! rusqlite connections. Enabled with the `typed-sqlite` feature.

use std::sync::Arc;

use bb8::Pool;
use bb8_rusqlite::RusqliteConnectionManager;
use rusqlite;
use tokio::sync::Mutex;

use crate::executor::QueryTarget;
use crate::middleware::{QueryBuilder, RowValues, SqlMiddlewareDbError};

/// Marker type for an idle connection.
pub enum Idle {}
/// Marker type for a connection inside an explicit transaction.
pub enum InTx {}

/// Shared handle to a rusqlite connection guarded by a mutex for async access.
type SharedConn = Arc<Mutex<rusqlite::Connection>>;

/// Typestate connection wrapper (SQLite-only prototype).
pub struct Connection<State> {
    conn: SharedConn,
    _state: std::marker::PhantomData<State>,
}

impl Connection<Idle> {
    /// Build a connection wrapper from a bb8 pool (sqlite-only).
    pub async fn from_pool(
        pool: &Pool<RusqliteConnectionManager>,
    ) -> Result<Self, SqlMiddlewareDbError> {
        let raw = pool.get().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("bb8 sqlite checkout error: {e}"))
        })?;
        Ok(Self {
            conn: Arc::new(Mutex::new(raw.into_inner())),
            _state: std::marker::PhantomData,
        })
    }

    /// Begin a transaction, consuming the idle state.
    pub async fn begin(self) -> Result<Connection<InTx>, SqlMiddlewareDbError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            guard
                .execute_batch("BEGIN")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
        .map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("sqlite begin join error: {e}"))
        })??;

        Ok(Connection {
            conn: self.conn,
            _state: std::marker::PhantomData,
        })
    }

    /// Run a batch with implicit BEGIN/COMMIT (auto-commit).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let conn = self.conn.clone();
        let sql_owned = sql.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            guard
                .execute_batch(&format!("BEGIN; {sql_owned}; COMMIT;"))
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("sqlite batch join error: {e}")))?
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(&self.conn, false), sql)
    }
}

impl Connection<InTx> {
    /// Commit the transaction, returning to idle state.
    pub async fn commit(self) -> Result<Connection<Idle>, SqlMiddlewareDbError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            guard
                .execute_batch("COMMIT")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("sqlite commit join error: {e}")))??;

        Ok(Connection {
            conn: self.conn,
            _state: std::marker::PhantomData,
        })
    }

    /// Roll back the transaction, returning to idle state.
    pub async fn rollback(self) -> Result<Connection<Idle>, SqlMiddlewareDbError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            guard
                .execute_batch("ROLLBACK")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("sqlite rollback join error: {e}")))??;

        Ok(Connection {
            conn: self.conn,
            _state: std::marker::PhantomData,
        })
    }

    /// Run a batch inside the open transaction (caller controls commit/rollback).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let conn = self.conn.clone();
        let sql_owned = sql.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            guard
                .execute_batch(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("sqlite tx batch join error: {e}")))?
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(&self.conn, true), sql)
    }
}

pub(crate) async fn typed_sqlite_select(
    conn: &SharedConn,
    query: &str,
    params: &[RowValues],
) -> Result<crate::results::ResultSet, SqlMiddlewareDbError> {
    let conn = conn.clone();
    let sql_owned = query.to_owned();
    let params_owned = params.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut guard = conn.blocking_lock();
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        let converted = crate::sqlite::params::Params::convert(&params_owned)?;
        crate::sqlite::query::build_result_set(&mut stmt, &converted.0)
    })
    .await
    .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("sqlite select join error: {e}")))?
}

pub(crate) async fn typed_sqlite_dml(
    conn: &SharedConn,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let conn = conn.clone();
    let sql_owned = query.to_owned();
    let params_owned = params.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut guard = conn.blocking_lock();
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        let converted = crate::sqlite::params::Params::convert(&params_owned)?;
        let affected = stmt
            .execute(converted.0.as_slice())
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "sqlite affected rows conversion error: {e}"
            ))
        })
    })
    .await
    .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("sqlite dml join error: {e}")))?
}
