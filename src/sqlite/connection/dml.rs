use std::sync::Arc;

use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, SqlMiddlewareDbError};
use crate::types::RowValues;

use super::{SqliteConnection, run_blocking};
use crate::sqlite::config::SqliteManager;
use crate::sqlite::params::Params;
use bb8::PooledConnection;

impl SqliteConnection {
    /// Execute a batch of statements; wraps in a transaction when not already inside one.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if acquiring the `SQLite` guard or executing the batch fails.
    pub async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        self.ensure_not_in_tx("execute batch")?;
        let sql_owned = query.to_owned();
        run_blocking(self.conn_handle(), move |guard| {
            if guard.is_autocommit() {
                let tx = guard
                    .transaction()
                    .map_err(SqlMiddlewareDbError::SqliteError)?;
                tx.execute_batch(&sql_owned)
                    .map_err(SqlMiddlewareDbError::SqliteError)?;
                tx.commit().map_err(SqlMiddlewareDbError::SqliteError)
            } else {
                guard
                    .execute_batch(&sql_owned)
                    .map_err(SqlMiddlewareDbError::SqliteError)
            }
        })
        .await
    }

    /// Execute a DML statement and return rows affected.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if preparing or executing the statement fails.
    pub async fn execute_dml(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.ensure_not_in_tx("execute dml")?;
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare_cached(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            let refs: Vec<&dyn rusqlite::ToSql> = params_owned
                .iter()
                .map(|v| v as &dyn rusqlite::ToSql)
                .collect();
            let affected = stmt
                .execute(&refs[..])
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            Ok(affected)
        })
        .await
    }

    /// Execute a DML statement inside an open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the guard fails or the transaction is not active.
    pub async fn execute_dml_in_tx(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
    ) -> Result<usize, SqlMiddlewareDbError> {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare_cached(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            let refs: Vec<&dyn rusqlite::ToSql> = params_owned
                .iter()
                .map(|v| v as &dyn rusqlite::ToSql)
                .collect();
            let affected = stmt
                .execute(&refs[..])
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            Ok(affected)
        })
        .await
    }

    /// Execute a batch inside an open transaction without implicit commit.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the guard fails or the transaction is not active.
    pub async fn execute_batch_in_tx(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let sql_owned = sql.to_owned();
        run_blocking(self.conn_handle(), move |guard| {
            guard
                .execute_batch(&sql_owned)
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
    conn: &mut PooledConnection<'static, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = convert_params::<Params>(params, ConversionMode::Execute)?.0;
    let sql_owned = query.to_owned();
    let params_owned = converted.clone();
    let handle = Arc::clone(&*conn);
    run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare_cached(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params_owned
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();
        let affected = stmt
            .execute(&refs[..])
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        Ok(affected)
    })
    .await
}
