use std::sync::Arc;

use crate::executor::QueryTarget;
use crate::middleware::{ResultSet, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::types::RowValues;

use crate::sqlite::config::SqliteManager;
use crate::sqlite::params::Params;
use super::{SqliteConnection, run_blocking};

impl SqliteConnection {
    /// Execute a SELECT and materialize into a `ResultSet`.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if preparing or executing the query fails.
    pub async fn execute_select<F>(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
        builder: F,
    ) -> Result<ResultSet, SqlMiddlewareDbError>
    where
        F: FnOnce(
                &mut rusqlite::Statement<'_>,
                &[rusqlite::types::Value],
            ) -> Result<ResultSet, SqlMiddlewareDbError>
            + Send
            + 'static,
    {
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            builder(&mut stmt, &params_owned)
        })
        .await
    }

    /// Execute a query inside an open transaction and build a `ResultSet`.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if preparing/executing the query fails or the transaction is not active.
    pub async fn execute_select_in_tx<F>(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
        builder: F,
    ) -> Result<ResultSet, SqlMiddlewareDbError>
    where
        F: FnOnce(
                &mut rusqlite::Statement<'_>,
                &[rusqlite::types::Value],
            ) -> Result<ResultSet, SqlMiddlewareDbError>
            + Send
            + 'static,
    {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            builder(&mut stmt, &params_owned)
        })
        .await
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(&mut self.conn, false), sql)
    }
}

/// Adapter for query builder select (typed-sqlite target).
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if converting parameters or executing the query fails.
pub async fn select(
    conn: &mut bb8::PooledConnection<'static, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = Params::convert(params)?.0;
    let sql_owned = query.to_owned();
    let params_owned = converted.clone();
    let handle = Arc::clone(&*conn);
    run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        super::super::query::build_result_set(&mut stmt, &params_owned)
    })
    .await
}
