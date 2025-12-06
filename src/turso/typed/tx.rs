use tokio::runtime::Handle;

use crate::middleware::SqlMiddlewareDbError;

use super::core::{InTx, Idle, SKIP_DROP_ROLLBACK};
use super::{TursoConnection, TursoManager};

impl TursoConnection<Idle> {
    /// Begin an explicit transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if transitioning into a transaction fails.
    pub async fn begin(mut self) -> Result<TursoConnection<InTx>, SqlMiddlewareDbError> {
        begin_from_conn(self.take_conn()?).await
    }
}

impl TursoConnection<InTx> {
    /// Commit and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if committing fails.
    pub async fn commit(mut self) -> Result<TursoConnection<Idle>, SqlMiddlewareDbError> {
        let conn = self.take_conn()?;
        match conn.execute_batch("COMMIT").await {
            Ok(()) => Ok(TursoConnection {
                conn: Some(conn),
                needs_rollback: false,
                _state: std::marker::PhantomData,
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
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if rolling back fails.
    pub async fn rollback(mut self) -> Result<TursoConnection<Idle>, SqlMiddlewareDbError> {
        let conn = self.take_conn()?;
        match conn.execute_batch("ROLLBACK").await {
            Ok(()) => Ok(TursoConnection {
                conn: Some(conn),
                needs_rollback: false,
                _state: std::marker::PhantomData,
            }),
            Err(e) => {
                self.conn = Some(conn);
                Err(SqlMiddlewareDbError::ExecutionError(format!(
                    "turso rollback error: {e}"
                )))
            }
        }
    }
}

pub(crate) async fn begin_from_conn(
    conn: bb8::PooledConnection<'static, TursoManager>,
) -> Result<TursoConnection<InTx>, SqlMiddlewareDbError> {
    conn.execute_batch("BEGIN")
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso begin error: {e}")))?;
    Ok(TursoConnection {
        conn: Some(conn),
        needs_rollback: true,
        _state: std::marker::PhantomData,
    })
}

fn skip_drop_rollback() -> bool {
    SKIP_DROP_ROLLBACK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Test-only escape hatch to simulate legacy behavior where dropping an in-flight transaction
/// leaked the transaction back to the pool. Do not use outside tests.
#[doc(hidden)]
pub fn set_skip_drop_rollback_for_tests(skip: bool) {
    SKIP_DROP_ROLLBACK.store(skip, std::sync::atomic::Ordering::Relaxed);
}

impl<State> Drop for TursoConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback
            && !skip_drop_rollback()
            && let Some(conn) = self.conn.take()
            && let Ok(handle) = Handle::try_current()
        {
            handle.spawn(async move {
                let _ = conn.execute_batch("ROLLBACK").await;
            });
        }
    }
}
