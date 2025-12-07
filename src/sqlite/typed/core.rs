use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use bb8::{Pool, PooledConnection};

use crate::middleware::SqlMiddlewareDbError;

use crate::sqlite::config::{SharedSqliteConnection, SqliteManager};

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// Typestate wrapper around a pooled `SQLite` connection.
///
/// When in `InTx` state, dropping without calling [`commit`](SqliteTypedConnection::<InTx>::commit)
/// or [`rollback`](SqliteTypedConnection::<InTx>::rollback) will trigger a best-effort synchronous
/// rollback in `Drop` to keep the pool clean. Prefer finishing transactions explicitly to avoid
/// surprise blocking work during drop.
pub struct SqliteTypedConnection<State> {
    pub(crate) conn: Option<PooledConnection<'static, SqliteManager>>,
    /// True if in a transaction that needs rollback on drop.
    pub(crate) needs_rollback: bool,
    pub(crate) _state: PhantomData<State>,
}

impl SqliteTypedConnection<Idle> {
    /// Checkout a connection from the pool.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if acquiring a pooled connection fails.
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
}

impl<State> SqliteTypedConnection<State> {
    pub(crate) fn take_conn(
        &mut self,
    ) -> Result<PooledConnection<'static, SqliteManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("sqlite connection already taken".into())
        })
    }

    pub(crate) fn conn_mut(&mut self) -> &mut PooledConnection<'static, SqliteManager> {
        self.conn.as_mut().expect("sqlite connection already taken")
    }

    pub(crate) fn conn_handle(&self) -> Result<SharedSqliteConnection, SqlMiddlewareDbError> {
        self.conn.as_ref().map(|c| Arc::clone(&**c)).ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("sqlite connection already taken".into())
        })
    }
}

pub(crate) fn in_tx(conn: PooledConnection<'static, SqliteManager>) -> SqliteTypedConnection<InTx> {
    SqliteTypedConnection {
        conn: Some(conn),
        needs_rollback: true,
        _state: PhantomData,
    }
}

pub(crate) async fn begin_from_conn(
    conn: PooledConnection<'static, SqliteManager>,
) -> Result<SqliteTypedConnection<InTx>, SqlMiddlewareDbError> {
    run_blocking(Arc::clone(&*conn), |guard| {
        guard
            .execute_batch("BEGIN")
            .map_err(SqlMiddlewareDbError::SqliteError)
    })
    .await?;
    Ok(in_tx(conn))
}

pub(crate) async fn run_blocking<F, R>(
    conn: SharedSqliteConnection,
    func: F,
) -> Result<R, SqlMiddlewareDbError>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    conn.execute(move |conn| {
        let _ = tx.send(func(conn));
    })?;
    rx.await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("sqlite worker receive error: {e}"))
    })?
}

pub(crate) static SKIP_DROP_ROLLBACK: AtomicBool = AtomicBool::new(false);
