use bb8::PooledConnection;

use crate::middleware::{RowValues, SqlMiddlewareDbError};

use super::{MssqlManager, MssqlTypedConnection};

impl MssqlTypedConnection<super::core::Idle> {
    /// Auto-commit batch.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the batch execution fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        crate::mssql::executor::execute_batch(self.conn_mut(), sql).await
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
        crate::mssql::executor::execute_dml(self.conn_mut(), query, params).await
    }
}

impl MssqlTypedConnection<super::core::InTx> {
    /// Execute batch inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the batch fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        crate::mssql::executor::execute_batch(self.conn_mut(), sql).await
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
        crate::mssql::executor::execute_dml(self.conn_mut(), query, params).await
    }
}

/// Adapter for query builder dml (typed-mssql target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if executing the DML fails.
pub async fn dml(
    conn: &mut PooledConnection<'_, MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    crate::mssql::executor::execute_dml(conn, query, params).await
}
