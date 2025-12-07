use std::{future::Future, marker::PhantomData, mem::ManuallyDrop, sync::atomic::AtomicBool};

use bb8::{ManageConnection, Pool, PooledConnection};

use crate::middleware::SqlMiddlewareDbError;

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// bb8 manager for Turso connections.
pub struct TursoManager {
    pub(crate) db: turso::Database,
}

impl TursoManager {
    #[must_use]
    pub fn new(db: turso::Database) -> Self {
        Self { db }
    }

    /// Build a pool from this manager.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if creating the pool fails.
    pub async fn build_pool(self) -> Result<Pool<TursoManager>, SqlMiddlewareDbError> {
        Pool::builder()
            .build(self)
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("turso pool error: {e}")))
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
///
/// In `InTx`, dropping without calling [`commit`](TursoConnection::<InTx>::commit) or
/// [`rollback`](TursoConnection::<InTx>::rollback) will spawn a best-effort rollback on the
/// runtime. Finish transactions explicitly to avoid silent background rollbacks and to surface
/// errors promptly.
pub struct TursoConnection<State> {
    pub(crate) conn: Option<PooledConnection<'static, TursoManager>>,
    /// True when a transaction is in-flight and needs rollback if dropped.
    pub(crate) needs_rollback: bool,
    pub(crate) _state: PhantomData<State>,
}

impl TursoConnection<Idle> {
    /// Checkout a connection from the pool.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if acquiring a connection fails.
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
}

impl<State> TursoConnection<State> {
    pub(crate) fn take_conn(
        &mut self,
    ) -> Result<PooledConnection<'static, TursoManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("turso connection already taken".into())
        })
    }

    pub(crate) fn conn_mut(&mut self) -> &mut PooledConnection<'static, TursoManager> {
        self.conn.as_mut().expect("turso connection already taken")
    }

    pub(crate) fn into_conn(mut self) -> Option<PooledConnection<'static, TursoManager>> {
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

pub(crate) static SKIP_DROP_ROLLBACK: AtomicBool = AtomicBool::new(false);
