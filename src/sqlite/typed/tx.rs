use std::sync::Arc;
use std::sync::atomic::Ordering;

use tokio::runtime::Handle;
use tokio::task::block_in_place;

use crate::middleware::SqlMiddlewareDbError;

use super::SqliteTypedConnection;
use super::core::{SKIP_DROP_ROLLBACK, begin_from_conn, run_blocking};
use crate::sqlite::connection::rollback_with_busy_retries;
use crate::sqlite::config::SharedSqliteConnection;

impl SqliteTypedConnection<super::core::Idle> {
    /// Begin an explicit transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if transitioning into a transaction fails.
    pub async fn begin(
        mut self,
    ) -> Result<SqliteTypedConnection<super::core::InTx>, SqlMiddlewareDbError> {
        begin_from_conn(self.take_conn()?).await
    }
}

impl SqliteTypedConnection<super::core::InTx> {
    /// Commit and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if committing the transaction fails.
    pub async fn commit(
        mut self,
    ) -> Result<SqliteTypedConnection<super::core::Idle>, SqlMiddlewareDbError> {
        let conn_handle = self.conn_handle()?;
        let commit_result = run_blocking(Arc::clone(&conn_handle), |guard| {
            guard
                .execute_batch("COMMIT")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await;

        match commit_result {
            Ok(()) => {
                let conn = self.take_conn()?;
                Ok(SqliteTypedConnection {
                    conn: Some(conn),
                    needs_rollback: false,
                    _state: std::marker::PhantomData,
                })
            }
            Err(err) => {
                // Best-effort rollback; keep needs_rollback = true so Drop can retry if needed.
                if rollback_with_busy_retries(Arc::clone(&conn_handle)).is_err() {
                    conn_handle.mark_broken();
                }
                Err(err)
            }
        }
    }

    /// Rollback and return to idle.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if rolling back the transaction fails.
    pub async fn rollback(
        mut self,
    ) -> Result<SqliteTypedConnection<super::core::Idle>, SqlMiddlewareDbError> {
        let conn_handle = self.conn_handle()?;
        let rollback_result = rollback_with_busy_retries(Arc::clone(&conn_handle));

        match rollback_result {
            Ok(()) => {
                let conn = self.take_conn()?;
                Ok(SqliteTypedConnection {
                    conn: Some(conn),
                    needs_rollback: false,
                    _state: std::marker::PhantomData,
                })
            }
            Err(err) => {
                // Keep connection + needs_rollback so Drop can attempt cleanup.
                conn_handle.mark_broken();
                Err(err)
            }
        }
    }
}

impl<State> Drop for SqliteTypedConnection<State> {
    fn drop(&mut self) {
        if self.needs_rollback
            && !skip_drop_rollback()
            && let Some(conn) = self.conn.take()
        {
            let conn_handle: SharedSqliteConnection = Arc::clone(&*conn);
            // Rollback synchronously so the connection is clean before it
            // goes back into the pool. Avoid async fire-and-forget, which
            // could race with the next checkout.
            let rollback = || rollback_with_busy_retries(Arc::clone(&conn_handle));
            let result = if Handle::try_current().is_ok() {
                block_in_place(rollback)
            } else {
                rollback()
            };

            if result.is_err() {
                conn_handle.mark_broken();
            }
        }
    }
}

fn skip_drop_rollback() -> bool {
    SKIP_DROP_ROLLBACK.load(Ordering::Relaxed)
}

/// Test-only escape hatch to simulate legacy behavior where dropping an in-flight transaction
/// leaked the transaction back to the pool. Do not use outside tests.
#[doc(hidden)]
pub fn set_skip_drop_rollback_for_tests(skip: bool) {
    SKIP_DROP_ROLLBACK.store(skip, Ordering::Relaxed);
}
