use bb8::PooledConnection;

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;

use super::{MssqlManager, MssqlTypedConnection};

impl MssqlTypedConnection<super::core::Idle> {
    /// Auto-commit SELECT.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        crate::mssql::executor::execute_select(self.conn_mut(), query, params).await
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_mssql(self.conn_mut(), false), sql)
    }
}

impl MssqlTypedConnection<super::core::InTx> {
    /// Execute SELECT inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        crate::mssql::executor::execute_select(self.conn_mut(), query, params).await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_mssql(self.conn_mut(), true), sql)
    }
}

/// Adapter for query builder select (typed-mssql target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if executing the select fails.
pub async fn select(
    conn: &mut PooledConnection<'_, MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    crate::mssql::executor::execute_select(conn, query, params).await
}
