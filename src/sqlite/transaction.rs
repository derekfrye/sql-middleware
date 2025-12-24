use std::sync::Arc;

use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};
use crate::pool::MiddlewarePoolConnection;
use crate::tx_outcome::TxOutcome;

use super::connection::SqliteConnection;
use super::params::Params;

use std::sync::atomic::{AtomicBool, Ordering};

static REWRAP_ON_ROLLBACK_FAILURE: AtomicBool = AtomicBool::new(false);

#[doc(hidden)]
pub fn set_rewrap_on_rollback_failure_for_tests(rewrap: bool) {
    REWRAP_ON_ROLLBACK_FAILURE.store(rewrap, Ordering::Relaxed);
}

fn rewrap_on_rollback_failure_for_tests() -> bool {
    REWRAP_ON_ROLLBACK_FAILURE.load(Ordering::Relaxed)
}

/// Transaction handle that owns the `SQLite` connection until completion.
pub struct Tx<'a> {
    conn: Option<SqliteConnection>,
    conn_slot: &'a mut MiddlewarePoolConnection,
}

/// Prepared statement tied to a `SQLite` transaction.
pub struct Prepared {
    sql: Arc<String>,
}

/// Begin a transaction, temporarily taking ownership of the pooled `SQLite` connection
/// until commit/rollback (or drop) returns it to the wrapper.
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if the transaction cannot be started.
pub async fn begin_transaction(
    conn_slot: &mut MiddlewarePoolConnection,
) -> Result<Tx<'_>, SqlMiddlewareDbError> {
    let conn = match conn_slot {
        MiddlewarePoolConnection::Sqlite { conn, .. } => conn,
        #[cfg(any(feature = "postgres", feature = "mssql", feature = "libsql", feature = "turso"))]
        _ => {
            return Err(SqlMiddlewareDbError::Unimplemented(
                "begin_transaction is only available for SQLite connections".into(),
            ));
        }
    };

    let mut conn = conn.take().ok_or_else(|| {
        SqlMiddlewareDbError::ExecutionError(
            "SQLite connection already taken from pool wrapper".into(),
        )
    })?;
    conn.begin().await?;
    Ok(Tx {
        conn: Some(conn),
        conn_slot,
    })
}

impl Tx<'_> {
    fn conn_mut(&mut self) -> Result<&mut SqliteConnection, SqlMiddlewareDbError> {
        self.conn.as_mut().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("SQLite transaction already completed".into())
        })
    }

    /// Prepare a statement within this transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the transaction has already completed.
    pub fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        if self.conn.is_none() {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction already completed".into(),
            ));
        }
        Ok(Prepared {
            sql: Arc::new(sql.to_owned()),
        })
    }

    /// Execute a prepared statement as DML within this transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if parameter conversion or execution fails.
    pub async fn execute_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
        let conn = self.conn_mut()?;
        conn.execute_dml_in_tx(prepared.sql.as_ref(), &converted.0)
            .await
    }

    /// Execute a prepared statement as a query within this transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if parameter conversion or execution fails.
    pub async fn query_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;
        let conn = self.conn_mut()?;
        conn.execute_select_in_tx(
            prepared.sql.as_ref(),
            &converted.0,
            super::query::build_result_set,
        )
        .await
    }

    /// Execute a batch inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the batch fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let conn = self.conn_mut()?;
        conn.execute_batch_in_tx(sql).await
    }

    /// Commit the transaction and rewrap the pooled connection.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if committing the transaction fails.
    pub async fn commit(mut self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        let mut conn = self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("SQLite transaction already completed".into())
        })?;
        match conn.commit().await {
            Ok(()) => {
                self.rewrap(conn);
                Ok(TxOutcome::without_restored_connection())
            }
            Err(err) => {
                let handle = conn.conn_handle();
                let rollback_result =
                    super::connection::rollback_with_busy_retries(handle.clone());
                if rollback_result.is_ok() {
                    conn.in_transaction = false;
                    self.rewrap(conn);
                } else if rewrap_on_rollback_failure_for_tests() {
                    conn.in_transaction = false;
                    self.rewrap(conn);
                    return Err(err);
                } else {
                    handle.mark_broken();
                }
                Err(err)
            }
        }
    }

    /// Roll back the transaction and rewrap the pooled connection.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if rolling back fails.
    pub async fn rollback(mut self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        let mut conn = self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("SQLite transaction already completed".into())
        })?;
        let handle = conn.conn_handle();
        match super::connection::rollback_with_busy_retries(handle.clone()) {
            Ok(()) => {
                conn.in_transaction = false;
                self.rewrap(conn);
                Ok(TxOutcome::without_restored_connection())
            }
            Err(err) => {
                if rewrap_on_rollback_failure_for_tests() {
                    conn.in_transaction = false;
                    self.rewrap(conn);
                    return Err(err);
                } else {
                    handle.mark_broken();
                    Err(err)
                }
            }
        }
    }

    fn rewrap(&mut self, conn: SqliteConnection) {
        let slot = match self.conn_slot {
            MiddlewarePoolConnection::Sqlite { conn: slot, .. } => slot,
            #[cfg(any(feature = "postgres", feature = "mssql", feature = "libsql", feature = "turso"))]
            _ => return,
        };
        debug_assert!(slot.is_none(), "sqlite conn slot should be empty during tx");
        *slot = Some(conn);
    }
}

impl Drop for Tx<'_> {
    /// Rolls back on drop to avoid leaking open transactions; the rollback is best-effort and
    /// SQLite may report "no transaction is active" if the transaction was already completed
    /// by user code (e.g., via `execute_batch_in_tx`). Such errors are ignored because the goal
    /// is simply to leave the connection in a clean state before returning it to the pool.
    fn drop(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            let handle = conn.conn_handle();
            let rollback_result = super::connection::rollback_with_busy_retries(handle.clone());
            if rollback_result.is_ok() {
                conn.in_transaction = false;
                self.rewrap(conn);
            } else if rewrap_on_rollback_failure_for_tests() {
                conn.in_transaction = false;
                self.rewrap(conn);
            } else {
                // Mark broken so the pool will drop and replace this connection instead of
                // handing out one that might still be mid-transaction.
                handle.mark_broken();
            }
        }
    }
}
