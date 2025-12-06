//! Experimental bb8-backed Postgres typestate API.
//! Provides `PgConnection<Idle>` / `PgConnection<InTx>` using an owned client
//! and explicit BEGIN/COMMIT/ROLLBACK.

use std::{
    future::Future,
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

use bb8::{ManageConnection, Pool, PooledConnection};
use tokio_postgres::{Client, NoTls};

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::postgres::query::{execute_dml_on_client, execute_query_on_client};
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// bb8 manager for Postgres clients.
pub struct PgManager {
    config: tokio_postgres::Config,
}

impl PgManager {
    #[must_use]
    pub fn new(config: tokio_postgres::Config) -> Self {
        Self { config }
    }
}

impl ManageConnection for PgManager {
    type Connection = Client;
    type Error = tokio_postgres::Error;

    #[allow(clippy::manual_async_fn)]
    fn connect(&self) -> impl Future<Output = Result<Self::Connection, Self::Error>> + Send {
        let cfg = self.config.clone();
        async move {
            let debug = std::env::var_os("SQL_MIDDLEWARE_PG_DEBUG").is_some();
            if debug {
                eprintln!(
                    "[sql-mw][pg] connect start hosts={:?} db={:?} user={:?}",
                    cfg.get_hosts(),
                    cfg.get_dbname(),
                    cfg.get_user()
                );
            }
            let (client, connection) = cfg.connect(NoTls).await?;
            if debug {
                eprintln!("[sql-mw][pg] connect established");
            }
            tokio::spawn(async move {
                if let Err(_e) = connection.await {
                    // drop error
                }
            });
            Ok(client)
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn is_valid(
        &self,
        conn: &mut Self::Connection,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move { conn.simple_query("SELECT 1").await.map(|_| ()) }
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}

/// Typestate wrapper around a pooled Postgres client.
pub struct PgConnection<State> {
    conn: Option<PooledConnection<'static, PgManager>>,
    /// True when a transaction is in-flight and needs rollback if dropped.
    needs_rollback: bool,
    _state: PhantomData<State>,
}

impl PgManager {
    /// Build a pool from this manager.
    pub async fn build_pool(self) -> Result<Pool<PgManager>, SqlMiddlewareDbError> {
        Pool::builder()
            .build(self)
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("postgres pool error: {e}")))
    }
}

impl PgConnection<Idle> {
    /// Checkout a connection from the pool.
    pub async fn from_pool(pool: &Pool<PgManager>) -> Result<Self, SqlMiddlewareDbError> {
        let conn = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("postgres checkout error: {e}"))
        })?;
        Ok(Self::new(conn, false))
    }

    /// Begin an explicit transaction.
    pub async fn begin(mut self) -> Result<PgConnection<InTx>, SqlMiddlewareDbError> {
        let conn = self.take_conn()?;
        conn.simple_query("BEGIN").await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres begin error: {e}"))
        })?;
        Ok(PgConnection::new(conn, true))
    }

    /// Auto-commit batch (BEGIN/COMMIT around it).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        crate::postgres::executor::execute_batch(self.conn_mut(), sql).await
    }

    /// Auto-commit DML.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        crate::postgres::executor::execute_dml(self.conn_mut(), query, params).await
    }

    /// Auto-commit SELECT.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        crate::postgres::executor::execute_select(self.conn_mut(), query, params).await
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_postgres(self.conn_mut(), false), sql)
    }
}

impl PgConnection<InTx> {
    /// Commit and return to idle.
    pub async fn commit(self) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        self.finish_tx("COMMIT", "commit").await
    }

    /// Rollback and return to idle.
    pub async fn rollback(self) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        self.finish_tx("ROLLBACK", "rollback").await
    }

    /// Execute batch inside the open transaction.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn_mut().batch_execute(sql).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres tx batch error: {e}"))
        })
    }

    /// Execute DML inside the open transaction.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        execute_dml_on_client(self.conn_mut(), query, params, "postgres tx execute error").await
    }

    /// Execute SELECT inside the open transaction.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        execute_query_on_client(self.conn_mut(), query, params).await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_postgres(self.conn_mut(), true), sql)
    }
}

/// Adapter for query builder select (typed-postgres target).
pub async fn select(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    execute_query_on_client(conn, query, params).await
}

/// Adapter for query builder dml (typed-postgres target).
pub async fn dml(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    execute_dml_on_client(conn, query, params, "postgres dml error").await
}

impl PgConnection<InTx> {
    async fn finish_tx(
        mut self,
        sql: &str,
        action: &str,
    ) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        let conn = self.take_conn()?;
        match conn
            .simple_query(sql)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres {action} error: {e}")))
        {
            Ok(_) => {
                self.needs_rollback = false;
                Ok(PgConnection::new(conn, false))
            }
            Err(err) => {
                // Best-effort rollback; keep needs_rollback so Drop can retry.
                let _ = conn.simple_query("ROLLBACK").await;
                self.conn = Some(conn);
                Err(err)
            }
        }
    }
}

// NOTE: Cannot specialize Drop for PgConnection<InTx> in Rust.
// Users must explicitly call commit() or rollback() to finalize transactions.
// If dropped without finalizing, Postgres will auto-rollback when the connection
// is returned to the pool (standard Postgres behavior for uncommitted transactions).
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

impl<State> Drop for PgConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback && !skip_drop_rollback() {
            if let Some(conn) = self.conn.take() {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = conn.simple_query("ROLLBACK").await;
                    });
                }
            }
        }
    }
}

impl<State> PgConnection<State> {
    fn new(conn: PooledConnection<'static, PgManager>, needs_rollback: bool) -> Self {
        Self {
            conn: Some(conn),
            needs_rollback,
            _state: PhantomData,
        }
    }

    fn conn_mut(&mut self) -> &mut PooledConnection<'static, PgManager> {
        self.conn.as_mut().expect("postgres connection already taken")
    }

    fn take_conn(&mut self) -> Result<PooledConnection<'static, PgManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("postgres connection already taken".into())
        })
    }
}
