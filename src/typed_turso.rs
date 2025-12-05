//! Experimental bb8-backed Turso typestate API.
//! Provides `TursoConnection<Idle>` / `TursoConnection<InTx>` using an owned Turso connection
//! with explicit BEGIN/COMMIT/ROLLBACK.

use std::{future::Future, marker::PhantomData};

use bb8::{ManageConnection, Pool, PooledConnection};

use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::turso::params::Params as TursoParams;
use crate::types::{ConversionMode, ParamConverter};
use crate::executor::QueryTarget;

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// bb8 manager for Turso connections.
pub struct TursoManager {
    db: turso::Database,
}

impl TursoManager {
    #[must_use]
    pub fn new(db: turso::Database) -> Self {
        Self { db }
    }
}

impl ManageConnection for TursoManager {
    type Connection = turso::Connection;
    type Error = turso::Error;

    fn connect(&self) -> impl Future<Output = Result<Self::Connection, Self::Error>> + Send {
        let db = self.db.clone();
        async move {
            eprintln!("[typed_turso::connect] attempting db.connect()");
            let out = db.connect();
            eprintln!("[typed_turso::connect] db.connect() returned");
            out
        }
    }

    fn is_valid(
        &self,
        conn: &mut Self::Connection,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move {
            eprintln!("[typed_turso::is_valid] running SELECT 1");
            match conn.query("SELECT 1", ()).await {
                Ok(mut rows) => {
                    // Consume one row to keep the iterator clean.
                    let _ = rows.next().await;
                    eprintln!("[typed_turso::is_valid] SELECT 1 ok");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("[typed_turso::is_valid] SELECT 1 error: {e}");
                    Err(e)
                }
            }
        }
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}

/// Typestate wrapper around a pooled Turso connection.
pub struct TursoConnection<State> {
    conn: PooledConnection<'static, TursoManager>,
    _state: PhantomData<State>,
}

impl TursoManager {
    /// Build a pool from this manager.
    pub async fn build_pool(self) -> Result<Pool<TursoManager>, SqlMiddlewareDbError> {
        Pool::builder()
            .build(self)
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("turso pool error: {e}")))
    }
}

impl TursoConnection<Idle> {
    /// Checkout a connection from the pool.
    pub async fn from_pool(pool: &Pool<TursoManager>) -> Result<Self, SqlMiddlewareDbError> {
        eprintln!("[typed_turso::from_pool] waiting for pooled connection");
        let conn = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("turso checkout error: {e}"))
        })?;
        eprintln!("[typed_turso::from_pool] acquired pooled connection");
        Ok(Self {
            conn,
            _state: PhantomData,
        })
    }

    /// Begin an explicit transaction.
    pub async fn begin(self) -> Result<TursoConnection<InTx>, SqlMiddlewareDbError> {
        self.conn
            .execute_batch("BEGIN")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso begin error: {e}")))?;
        Ok(TursoConnection {
            conn: self.conn,
            _state: PhantomData,
        })
    }

    /// Auto-commit batch (BEGIN/COMMIT around it).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let script = format!("BEGIN; {sql}; COMMIT;");
        self.conn
            .execute_batch(&script)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso batch error: {e}")))
    }

    /// Auto-commit DML.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
        let rows = self
            .conn
            .execute(query, converted.0)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso execute error: {e}")))?;
        usize::try_from(rows).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "turso affected rows conversion error: {e}"
            ))
        })
    }

    /// Auto-commit SELECT.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<crate::results::ResultSet, SqlMiddlewareDbError> {
        // Reuse the shared Turso select path so column names are preserved in the `ResultSet`.
        crate::turso::execute_select(&self.conn, query, params).await
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_turso(&mut self.conn, false), sql)
    }
}

impl TursoConnection<InTx> {
    /// Commit and return to idle.
    pub async fn commit(self) -> Result<TursoConnection<Idle>, SqlMiddlewareDbError> {
        self.conn
            .execute_batch("COMMIT")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso commit error: {e}")))?;
        Ok(TursoConnection {
            conn: self.conn,
            _state: PhantomData,
        })
    }

    /// Rollback and return to idle.
    pub async fn rollback(self) -> Result<TursoConnection<Idle>, SqlMiddlewareDbError> {
        self.conn
            .execute_batch("ROLLBACK")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso rollback error: {e}")))?;
        Ok(TursoConnection {
            conn: self.conn,
            _state: PhantomData,
        })
    }

    /// Execute batch inside the open transaction.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn
            .execute_batch(sql)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso tx batch error: {e}")))
    }

    /// Execute DML inside the open transaction.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
        let rows = self
            .conn
            .execute(query, converted.0)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso tx execute error: {e}")))?;
        usize::try_from(rows).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "turso affected rows conversion error: {e}"
            ))
        })
    }

    /// Execute SELECT inside the open transaction.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<crate::results::ResultSet, SqlMiddlewareDbError> {
        crate::turso::execute_select(&self.conn, query, params).await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_turso(&mut self.conn, true), sql)
    }
}

/// Adapter for query builder select (typed-turso target).
pub async fn select(
    conn: &mut PooledConnection<'static, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<crate::results::ResultSet, SqlMiddlewareDbError> {
    crate::turso::execute_select(&*conn, query, params).await
}

/// Adapter for query builder dml (typed-turso target).
pub async fn dml(
    conn: &mut PooledConnection<'static, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
    let rows = conn
        .execute(query, converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso dml error: {e}")))?;
    usize::try_from(rows).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!(
            "turso affected rows conversion error: {e}"
        ))
    })
}
