use bb8::PooledConnection;

use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::turso::params::Params as TursoParams;
use crate::types::{ConversionMode, ParamConverter};

use super::{InTx, TursoConnection, TursoManager};

impl TursoConnection<super::core::Idle> {
    /// Auto-commit batch (BEGIN/COMMIT around it).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the batch fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let mut tx = super::tx::begin_from_conn(self.take_conn()?).await?;
        tx.execute_batch(sql).await?;
        let idle = tx.commit().await?;
        self.conn = idle.into_conn();
        Ok(())
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
        let mut tx = super::tx::begin_from_conn(self.take_conn()?).await?;
        let rows = tx.dml(query, params).await?;
        let idle = tx.commit().await?;
        self.conn = idle.into_conn();
        Ok(rows)
    }
}

impl TursoConnection<InTx> {
    /// Execute batch inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the batch execution fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn_mut()
            .execute_batch(sql)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso batch error: {e}")))
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
        let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
        let mut rows = self
            .conn_mut()
            .query(query, converted.0)
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("turso tx execute error: {e}"))
            })?;
        let mut count = 0usize;
        while rows.next().await.transpose().is_some() {
            count += 1;
        }
        Ok(count)
    }
}

/// Adapter for query builder dml (typed-turso target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if executing the DML fails.
pub async fn dml(
    conn: &mut PooledConnection<'_, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = TursoParams::convert_sql_params(params, ConversionMode::Execute)?;
    let mut rows = conn
        .query(query, converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso dml error: {e}")))?;

    let mut count = 0usize;
    while rows.next().await.transpose().is_some() {
        count += 1;
    }
    Ok(count)
}
