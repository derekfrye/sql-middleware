use bb8::PooledConnection;

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::query_utils::extract_column_names;
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;
use crate::adapters::params::convert_params;
use crate::turso::params::Params as TursoParams;
use crate::types::ConversionMode;

use super::{InTx, TursoConnection, TursoManager};

impl TursoConnection<super::core::Idle> {
    /// Auto-commit SELECT.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let mut tx = super::tx::begin_from_conn(self.take_conn()?).await?;
        let rows = tx.select(query, params).await?;
        let idle = tx.commit().await?;
        self.conn = idle.into_conn();
        Ok(rows)
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_turso(self.conn_mut(), false), sql)
    }
}

impl TursoConnection<InTx> {
    /// Execute SELECT inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        select_rows(self.conn_mut(), query, params).await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_turso(self.conn_mut(), true), sql)
    }
}

/// Adapter for query builder select (typed-turso target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if the query execution fails.
pub async fn select(
    conn: &mut PooledConnection<'_, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    select_rows(conn, query, params).await
}

/// Shared helper to run a SELECT against a pooled Turso client and build a `ResultSet`.
async fn select_rows(
    conn: &mut PooledConnection<'_, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = convert_params::<TursoParams>(params, ConversionMode::Query)?;
    let mut stmt = conn
        .prepare(query)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso prepare error: {e}")))?;

    let cols = extract_column_names(stmt.columns().iter(), |col| col.name());
    let cols_arc = std::sync::Arc::new(cols);

    let rows = stmt
        .query(converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("turso query error: {e}")))?;

    crate::turso::query::build_result_set(rows, Some(cols_arc)).await
}
