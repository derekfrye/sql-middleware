use std::sync::Arc;

use crate::executor::QueryTarget;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;

use crate::sqlite::config::SqliteManager;
use crate::sqlite::params::Params;
use crate::sqlite::query;

use super::SqliteTypedConnection;

impl SqliteTypedConnection<super::core::Idle> {
    /// Auto-commit SELECT.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let mut tx = super::core::begin_from_conn(self.take_conn()?).await?;
        let rows = tx.select(query, params).await?;
        let mut idle = tx.commit().await?;
        self.conn = idle.conn.take();
        Ok(rows)
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(self.conn_mut(), false), sql)
    }
}

impl SqliteTypedConnection<super::core::InTx> {
    /// Execute SELECT inside the open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if executing the select fails.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = Params::convert(params)?.0;
        let sql_owned = query.to_owned();
        super::core::run_blocking(self.conn_handle()?, move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            query::build_result_set(&mut stmt, &converted)
        })
        .await
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(self.conn_mut(), true), sql)
    }
}

/// Adapter for query builder select (typed-sqlite target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if converting parameters or executing the query fails.
pub async fn select(
    conn: &mut bb8::PooledConnection<'_, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = Params::convert(params)?.0;
    let sql_owned = query.to_owned();
    let handle = Arc::clone(&**conn);
    super::core::run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        query::build_result_set(&mut stmt, &converted)
    })
    .await
}
