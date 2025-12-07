use tokio::runtime::Handle;

use crate::middleware::SqlMiddlewareDbError;

use super::core::{Idle, InTx, PgConnection, SKIP_DROP_ROLLBACK};

impl PgConnection<Idle> {
    /// Begin an explicit transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if starting the transaction fails.
    pub async fn begin(mut self) -> Result<PgConnection<InTx>, SqlMiddlewareDbError> {
        let conn = self.take_conn()?;
        conn.simple_query("BEGIN").await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres begin error: {e}"))
        })?;
        Ok(PgConnection::new(conn, true))
    }
}

impl PgConnection<InTx> {
    /// Commit and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the commit fails.
    pub async fn commit(self) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        self.finish_tx("COMMIT", "commit").await
    }

    /// Rollback and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the rollback fails.
    pub async fn rollback(self) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        self.finish_tx("ROLLBACK", "rollback").await
    }

    async fn finish_tx(
        mut self,
        sql: &str,
        action: &str,
    ) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        let conn = self.take_conn()?;
        match conn.simple_query(sql).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres {action} error: {e}"))
        }) {
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
    SKIP_DROP_ROLLBACK.load(std::sync::atomic::Ordering::Relaxed)
}

impl<State> Drop for PgConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback
            && !skip_drop_rollback()
            && let Some(conn) = self.conn.take()
            && let Ok(handle) = Handle::try_current()
        {
            handle.spawn(async move {
                let _ = conn.simple_query("ROLLBACK").await;
            });
        }
    }
}

/// Test-only escape hatch to simulate legacy behavior where dropping an in-flight transaction
/// leaked the transaction back to the pool. Do not use outside tests.
#[doc(hidden)]
pub fn set_skip_drop_rollback_for_tests(skip: bool) {
    SKIP_DROP_ROLLBACK.store(skip, std::sync::atomic::Ordering::Relaxed);
}
