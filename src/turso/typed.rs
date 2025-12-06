//! Experimental bb8-backed Turso typestate API.
//! Provides `TursoConnection<Idle>` / `TursoConnection<InTx>` using an owned Turso connection
//! with explicit BEGIN/COMMIT/ROLLBACK.

use std::{
    future::Future,
    marker::PhantomData,
    mem::ManuallyDrop,
    sync::atomic::{AtomicBool, Ordering},
};

use bb8::{ManageConnection, Pool, PooledConnection};

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::turso::params::Params as TursoParams;
use crate::types::{ConversionMode, ParamConverter};

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

    #[allow(clippy::manual_async_fn)]
    fn connect(&self) -> impl Future<Output = Result<Self::Connection, Self::Error>> + Send {
        let db = self.db.clone();
        async move { db.connect() }
    }

    #[allow(clippy::manual_async_fn)]
    fn is_valid(
        &self,
        conn: &mut Self::Connection,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move {
            match conn.query("SELECT 1", ()).await {
                Ok(mut rows) => {
                    // Consume one row to keep the iterator clean.
                    let _ = rows.next().await;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}

/// Typestate wrapper around a pooled Turso connection.
pub struct TursoConnection<State> {
    conn: Option<PooledConnection<'static, TursoManager>>,
    /// True when a transaction is in-flight and needs rollback if dropped.
    needs_rollback: bool,
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
        let conn = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("turso checkout error: {e}"))
        })?;
        Ok(Self {
            conn: Some(conn),
            needs_rollback: false,
            _state: PhantomData,
        })
    }

    /// Begin an explicit transaction.
    pub async fn begin(mut self) -> Result<TursoConnection<InTx>, SqlMiddlewareDbError> {
        Self::begin_from_conn(self.take_conn()?).await
    }

    /// Auto-commit batch (BEGIN/COMMIT around it).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let mut tx = Self::begin_from_conn(self.take_conn()?).await?;
        tx.execute_batch(sql).await?;
        let idle = tx.commit().await?;
        self.conn = idle.into_conn();
        Ok(())
    }

    /// Auto-commit DML.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let mut tx = Self::begin_from_conn(self.take_conn()?).await?;
        let rows = tx.dml(query, params).await?;
        let idle = tx.commit().await?;
        self.conn = idle.into_conn();
        Ok(rows)
    }

    /// Auto-commit SELECT.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let mut tx = Self::begin_from_conn(self.take_conn()?).await?;
        let rows = tx.select(query, params).await?;
        let idle = tx.commit().await?;
        self.conn = idle.into_conn();
        Ok(rows)
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_turso(self.conn_mut(), false), sql)
    }
}

use crate::results::ResultSet;

impl TursoConnection<InTx> {
    /// Commit and return to idle.
    pub async fn commit(mut self) -> Result<TursoConnection<Idle>, SqlMiddlewareDbError> {
        let conn = self.take_conn_owned()?;
        match conn.execute_batch("COMMIT").await {
            Ok(_) => Ok(TursoConnection {
                conn: Some(conn),
                needs_rollback: false,
                _state: PhantomData,
            }),
            Err(e) => {
                // Best-effort rollback; keep needs_rollback so Drop can retry if needed.
                let _ = conn.execute_batch("ROLLBACK").await;
                self.conn = Some(conn);
                Err(SqlMiddlewareDbError::ExecutionError(format!(
                    "turso commit error: {e}"
                )))
            }
        }
    }

    /// Rollback and return to idle.
    pub async fn rollback(mut self) -> Result<TursoConnection<Idle>, SqlMiddlewareDbError> {
        let conn = self.take_conn_owned()?;
        match conn.execute_batch("ROLLBACK").await {
            Ok(_) => Ok(TursoConnection {
                conn: Some(conn),
                needs_rollback: false,
                _state: PhantomData,
            }),
            Err(e) => {
                self.conn = Some(conn);
                Err(SqlMiddlewareDbError::ExecutionError(format!(
                    "turso rollback error: {e}"
                )))
            }
        }
    }

    /// Execute batch inside the open transaction.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn_mut()
            .execute_batch(sql)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso batch error: {e}")))
    }

    /// Execute DML inside the open transaction.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
        let mut rows = self
            .conn_mut()
            .query(query, converted.0)
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("turso tx execute error: {e}"))
            })?;
        let mut count = 0usize;
        while rows.next().await.transpose().is_some() {
            count += 1;
        }
        Ok(count)
    }

    /// Execute SELECT inside the open transaction.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        select_rows(self.conn_mut(), query, params).await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_turso(self.conn_mut(), true), sql)
    }
}

/// Adapter for query builder select (typed-turso target).
pub async fn select(
    conn: &mut PooledConnection<'_, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    select_rows(conn, query, params).await
}

/// Adapter for query builder dml (typed-turso target).
pub async fn dml(
    conn: &mut PooledConnection<'_, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
    let mut rows = conn
        .query(query, converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso dml error: {e}")))?;

    let mut count = 0usize;
    while rows.next().await.transpose().is_some() {
        count += 1;
    }
    Ok(count)
}

/// Shared helper to run a SELECT against a pooled Turso client and build a `ResultSet`.
async fn select_rows(
    conn: &mut PooledConnection<'_, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = TursoParams::convert_sql_params(params, ConversionMode::Query)?;
    let mut stmt = conn
        .prepare(query)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso prepare error: {e}")))?;

    let cols = stmt
        .columns()
        .into_iter()
        .map(|c| c.name().to_string())
        .collect::<Vec<_>>();
    let cols_arc = std::sync::Arc::new(cols);

    let rows = stmt
        .query(converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso query error: {e}")))?;

    crate::turso::query::build_result_set(rows, Some(cols_arc)).await
}

impl TursoConnection<Idle> {
    fn take_conn(
        &mut self,
    ) -> Result<PooledConnection<'static, TursoManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("turso connection already taken".into())
        })
    }

    fn conn_mut(&mut self) -> &mut PooledConnection<'static, TursoManager> {
        self.conn.as_mut().expect("turso connection already taken")
    }

    fn in_tx(conn: PooledConnection<'static, TursoManager>) -> TursoConnection<InTx> {
        TursoConnection {
            conn: Some(conn),
            needs_rollback: true,
            _state: PhantomData,
        }
    }

    async fn begin_from_conn(
        conn: PooledConnection<'static, TursoManager>,
    ) -> Result<TursoConnection<InTx>, SqlMiddlewareDbError> {
        conn.execute_batch("BEGIN")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso begin error: {e}")))?;
        Ok(TursoConnection::in_tx(conn))
    }
}

impl TursoConnection<InTx> {
    fn take_conn_owned(
        &mut self,
    ) -> Result<PooledConnection<'static, TursoManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("turso connection already taken".into())
        })
    }

    fn conn_mut(&mut self) -> &mut PooledConnection<'static, TursoManager> {
        self.conn.as_mut().expect("turso connection already taken")
    }
}

fn skip_drop_rollback() -> bool {
    SKIP_DROP_ROLLBACK.load(Ordering::Relaxed)
}

static SKIP_DROP_ROLLBACK: AtomicBool = AtomicBool::new(false);

/// Test-only escape hatch to simulate legacy behavior where dropping an in-flight transaction
/// leaked the transaction back to the pool. Do not use outside tests.
#[doc(hidden)]
pub fn set_skip_drop_rollback_for_tests(skip: bool) {
    SKIP_DROP_ROLLBACK.store(skip, Ordering::Relaxed);
}

impl<State> Drop for TursoConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback && !skip_drop_rollback() {
            if let Some(conn) = self.conn.take() {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = conn.execute_batch("ROLLBACK").await;
                    });
                }
            }
        }
    }
}

impl<State> TursoConnection<State> {
    fn into_conn(mut self) -> Option<PooledConnection<'static, TursoManager>> {
        debug_assert!(
            !self.needs_rollback,
            "into_conn should only be called when rollback not needed"
        );
        let conn = self.conn.take();
        // Prevent Drop from running (which could try to rollback a finished tx).
        let _ = ManuallyDrop::new(self);
        conn
    }
}
