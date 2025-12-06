use bb8::PooledConnection;

use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::postgres::query::execute_dml_on_client;

use super::{PgConnection, PgManager};

impl PgConnection<super::core::Idle> {
    /// Auto-commit batch (BEGIN/COMMIT around it).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the batch execution fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        crate::postgres::executor::execute_batch(self.conn_mut(), sql).await
    }

    /// Auto-commit DML.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the DML fails.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        crate::postgres::executor::execute_dml(self.conn_mut(), query, params).await
    }
}

impl PgConnection<super::core::InTx> {
    /// Execute batch inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the batch fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn_mut().batch_execute(sql).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres tx batch error: {e}"))
        })
    }

    /// Execute DML inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the DML fails.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        execute_dml_on_client(self.conn_mut(), query, params, "postgres tx execute error").await
    }
}

/// Adapter for query builder dml (typed-postgres target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if executing the DML fails.
pub async fn dml(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    execute_dml_on_client(conn, query, params, "postgres dml error").await
}
