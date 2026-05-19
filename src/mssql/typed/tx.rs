use tiberius::Query;
use tokio::runtime::Handle;

use crate::middleware::SqlMiddlewareDbError;

use super::core::{Idle, InTx, MssqlTypedConnection, SKIP_DROP_ROLLBACK};

impl MssqlTypedConnection<Idle> {
    /// Begin an explicit transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if starting the transaction fails.
    pub async fn begin(mut self) -> Result<MssqlTypedConnection<InTx>, SqlMiddlewareDbError> {
        let mut conn = self.take_conn()?;
        Query::new("BEGIN TRANSACTION")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("MSSQL begin transaction error: {e}"))
            })?;
        Ok(MssqlTypedConnection::new(conn, true))
    }
}

impl MssqlTypedConnection<InTx> {
    /// Commit and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the commit fails.
    pub async fn commit(self) -> Result<MssqlTypedConnection<Idle>, SqlMiddlewareDbError> {
        self.finish_tx("COMMIT TRANSACTION", "commit").await
    }

    /// Rollback and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the rollback fails.
    pub async fn rollback(self) -> Result<MssqlTypedConnection<Idle>, SqlMiddlewareDbError> {
        self.finish_tx("ROLLBACK TRANSACTION", "rollback").await
    }

    async fn finish_tx(
        mut self,
        sql: &str,
        action: &str,
    ) -> Result<MssqlTypedConnection<Idle>, SqlMiddlewareDbError> {
        let mut conn = self.take_conn()?;
        match Query::new(sql)
            .execute(&mut *conn)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("MSSQL {action} error: {e}")))
        {
            Ok(_) => {
                self.needs_rollback = false;
                Ok(MssqlTypedConnection::new(conn, false))
            }
            Err(err) => {
                let _ = Query::new("ROLLBACK TRANSACTION").execute(&mut *conn).await;
                self.conn = Some(conn);
                Err(err)
            }
        }
    }
}

fn skip_drop_rollback() -> bool {
    SKIP_DROP_ROLLBACK.load(std::sync::atomic::Ordering::Relaxed)
}

impl<State> Drop for MssqlTypedConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback
            && !skip_drop_rollback()
            && let Some(mut conn) = self.conn.take()
            && let Ok(handle) = Handle::try_current()
        {
            handle.spawn(async move {
                let _ = Query::new("ROLLBACK TRANSACTION").execute(&mut *conn).await;
            });
        }
    }
}

/// Test-only escape hatch to simulate dropping an in-flight transaction without rollback.
#[doc(hidden)]
pub fn set_skip_drop_rollback_for_tests(skip: bool) {
    SKIP_DROP_ROLLBACK.store(skip, std::sync::atomic::Ordering::Relaxed);
}
