use std::sync::Arc;

use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, RowValues, SqlMiddlewareDbError};

use super::SqliteTypedConnection;
use crate::sqlite::config::SqliteManager;
use crate::sqlite::params::Params;

impl SqliteTypedConnection<super::core::Idle> {
    /// Auto-commit batch (BEGIN/COMMIT around it).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the batch fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let mut tx = super::core::begin_from_conn(self.take_conn()?).await?;
        tx.execute_batch(sql).await?;
        let mut idle = tx.commit().await?;
        self.conn = idle.conn.take();
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
        let mut tx = super::core::begin_from_conn(self.take_conn()?).await?;
        let rows = tx.dml(query, params).await?;
        let mut idle = tx.commit().await?;
        self.conn = idle.conn.take();
        Ok(rows)
    }
}

impl SqliteTypedConnection<super::core::InTx> {
    /// Execute batch inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the batch fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let sql_owned = sql.to_owned();
        super::core::run_blocking(self.conn_handle()?, move |guard| {
            guard
                .execute_batch(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
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
        let converted = convert_params::<Params>(params, ConversionMode::Execute)?.0;
        let sql_owned = query.to_owned();
        super::core::run_blocking(self.conn_handle()?, move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            let refs: Vec<&dyn rusqlite::ToSql> = converted
                .iter()
                .map(|v| v as &dyn rusqlite::ToSql)
                .collect();
            stmt.execute(&refs[..])
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
    }
}

/// Adapter for query builder dml (typed-sqlite target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if converting parameters or executing the statement fails.
pub async fn dml(
    conn: &mut bb8::PooledConnection<'_, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = convert_params::<Params>(params, ConversionMode::Execute)?.0;
    let sql_owned = query.to_owned();
    let handle = Arc::clone(&**conn);
    super::core::run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        let refs: Vec<&dyn rusqlite::ToSql> = converted
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();
        stmt.execute(&refs[..])
            .map_err(SqlMiddlewareDbError::SqliteError)
    })
    .await
}
