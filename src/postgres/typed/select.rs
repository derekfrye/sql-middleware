use bb8::PooledConnection;

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::postgres::query::execute_query_on_client;
use crate::postgres::query::execute_query_prepared_on_client;
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;

use super::{PgConnection, PgManager};

impl PgConnection<super::core::Idle> {
    /// Auto-commit SELECT.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        crate::postgres::executor::execute_select(self.conn_mut(), query, params).await
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(
            QueryTarget::from_typed_postgres(self.conn_mut(), false),
            sql,
        )
    }
}

impl PgConnection<super::core::InTx> {
    /// Execute SELECT inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        execute_query_on_client(self.conn_mut(), query, params).await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_postgres(self.conn_mut(), true), sql)
    }
}

/// Adapter for query builder select (typed-postgres target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if the query execution fails.
pub async fn select(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    execute_query_on_client(conn, query, params).await
}

/// Adapter for query builder select using prepared statements (typed-postgres target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if preparing or executing the query fails.
pub async fn select_prepared(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    execute_query_prepared_on_client(conn, query, params).await
}
