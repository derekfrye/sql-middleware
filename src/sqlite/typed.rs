//! Experimental bb8-backed SQLite typestate API.
//! Provides `SqliteTypedConnection<Idle>` / `SqliteTypedConnection<InTx>` using an owned
//! pooled SQLite connection with explicit BEGIN/COMMIT/ROLLBACK.

use std::marker::PhantomData;
use std::sync::Arc;

use bb8::{Pool, PooledConnection};
use tokio::task::spawn_blocking;

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;

use super::config::{SharedSqliteConnection, SqliteManager};
use super::params::Params;

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// Typestate wrapper around a pooled SQLite connection.
pub struct SqliteTypedConnection<State> {
    conn: Option<PooledConnection<'static, SqliteManager>>,
    /// True if in a transaction that needs rollback on drop.
    needs_rollback: bool,
    _state: PhantomData<State>,
}

impl SqliteTypedConnection<Idle> {
    /// Checkout a connection from the pool.
    pub async fn from_pool(pool: &Pool<SqliteManager>) -> Result<Self, SqlMiddlewareDbError> {
        let conn = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("sqlite checkout error: {e}"))
        })?;
        Ok(Self {
            conn: Some(conn),
            needs_rollback: false,
            _state: PhantomData,
        })
    }

    /// Begin an explicit transaction.
    pub async fn begin(mut self) -> Result<SqliteTypedConnection<InTx>, SqlMiddlewareDbError> {
        Self::begin_from_conn(self.take_conn()?).await
    }

    /// Auto-commit batch (BEGIN/COMMIT around it).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let mut tx = Self::begin_from_conn(self.take_conn()?).await?;
        tx.execute_batch(sql).await?;
        let mut idle = tx.commit().await?;
        self.conn = idle.conn.take();
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
        let mut idle = tx.commit().await?;
        self.conn = idle.conn.take();
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
        let mut idle = tx.commit().await?;
        self.conn = idle.conn.take();
        Ok(rows)
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(self.conn_mut(), false), sql)
    }

    fn take_conn(
        &mut self,
    ) -> Result<PooledConnection<'static, SqliteManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("sqlite connection already taken".into())
        })
    }

    fn conn_mut(&mut self) -> &mut PooledConnection<'static, SqliteManager> {
        self.conn.as_mut().expect("sqlite connection already taken")
    }

    fn in_tx(conn: PooledConnection<'static, SqliteManager>) -> SqliteTypedConnection<InTx> {
        SqliteTypedConnection {
            conn: Some(conn),
            needs_rollback: true,
            _state: PhantomData,
        }
    }

    async fn begin_from_conn(
        conn: PooledConnection<'static, SqliteManager>,
    ) -> Result<SqliteTypedConnection<InTx>, SqlMiddlewareDbError> {
        run_blocking(Arc::clone(&*conn), |guard| {
            guard
                .execute_batch("BEGIN")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        Ok(SqliteTypedConnection::in_tx(conn))
    }
}

impl SqliteTypedConnection<InTx> {
    /// Commit and return to idle.
    pub async fn commit(mut self) -> Result<SqliteTypedConnection<Idle>, SqlMiddlewareDbError> {
        self.needs_rollback = false; // Transaction finalized, no rollback needed
        let conn = self.take_conn_owned()?;
        run_blocking(Arc::clone(&*conn), |guard| {
            guard
                .execute_batch("COMMIT")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        Ok(SqliteTypedConnection {
            conn: Some(conn),
            needs_rollback: false,
            _state: PhantomData,
        })
    }

    /// Rollback and return to idle.
    pub async fn rollback(mut self) -> Result<SqliteTypedConnection<Idle>, SqlMiddlewareDbError> {
        self.needs_rollback = false; // Transaction finalized, no rollback needed
        let conn = self.take_conn_owned()?;
        run_blocking(Arc::clone(&*conn), |guard| {
            guard
                .execute_batch("ROLLBACK")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        Ok(SqliteTypedConnection {
            conn: Some(conn),
            needs_rollback: false,
            _state: PhantomData,
        })
    }

    /// Execute batch inside the open transaction.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let sql_owned = sql.to_owned();
        run_blocking(self.conn_handle()?, move |guard| {
            guard
                .execute_batch(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
    }

    /// Execute DML inside the open transaction.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = Params::convert(params)?.0;
        let sql_owned = query.to_owned();
        run_blocking(self.conn_handle()?, move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            let refs: Vec<&dyn rusqlite::ToSql> =
                converted.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            stmt.execute(&refs[..])
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
    }

    /// Execute SELECT inside the open transaction.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = Params::convert(params)?.0;
        let sql_owned = query.to_owned();
        run_blocking(self.conn_handle()?, move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            super::query::build_result_set(&mut stmt, &converted)
        })
        .await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(self.conn_mut(), true), sql)
    }

    fn take_conn_owned(
        &mut self,
    ) -> Result<PooledConnection<'static, SqliteManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("sqlite connection already taken".into())
        })
    }

    fn conn_mut(&mut self) -> &mut PooledConnection<'static, SqliteManager> {
        self.conn.as_mut().expect("sqlite connection already taken")
    }

    fn conn_handle(&self) -> Result<SharedSqliteConnection, SqlMiddlewareDbError> {
        self.conn
            .as_ref()
            .map(|c| Arc::clone(&**c))
            .ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError("sqlite connection already taken".into())
            })
    }
}

impl<State> Drop for SqliteTypedConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback
            && let Some(conn) = self.conn.take()
            && let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            let conn_handle = Arc::clone(&*conn);
            handle.spawn(async move {
                let _ = spawn_blocking(move || {
                    let guard = conn_handle.blocking_lock();
                    let _ = guard.execute_batch("ROLLBACK");
                })
                .await;
            });
        }
    }
}

/// Adapter for query builder select (typed-sqlite target).
pub async fn select(
    conn: &mut PooledConnection<'_, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = Params::convert(params)?.0;
    let sql_owned = query.to_owned();
    let handle = Arc::clone(&**conn);
    run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        super::query::build_result_set(&mut stmt, &converted)
    })
    .await
}

/// Adapter for query builder dml (typed-sqlite target).
pub async fn dml(
    conn: &mut PooledConnection<'_, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = Params::convert(params)?.0;
    let sql_owned = query.to_owned();
    let handle = Arc::clone(&**conn);
    run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            converted.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        stmt.execute(&refs[..])
            .map_err(SqlMiddlewareDbError::SqliteError)
    })
    .await
}

async fn run_blocking<F, R>(
    conn: SharedSqliteConnection,
    func: F,
) -> Result<R, SqlMiddlewareDbError>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
    R: Send + 'static,
{
    spawn_blocking(move || {
        let mut guard = conn.blocking_lock();
        func(&mut guard)
    })
    .await
    .map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("sqlite spawn_blocking join error: {e}"))
    })?
}
