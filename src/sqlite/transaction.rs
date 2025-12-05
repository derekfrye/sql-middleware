use std::sync::Arc;

use crate::middleware::{ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError};

use super::connection::SqliteConnection;
use super::params::Params;

/// Transaction handle that owns the SQLite connection until completion.
pub struct Tx {
    conn: Option<SqliteConnection>,
}

/// Prepared statement tied to a `SQLite` transaction.
pub struct Prepared {
    sql: Arc<String>,
}

/// Begin a transaction, consuming the SQLite connection until commit/rollback.
pub async fn begin_transaction(mut conn: SqliteConnection) -> Result<Tx, SqlMiddlewareDbError> {
    conn.begin().await?;
    Ok(Tx { conn: Some(conn) })
}

impl Tx {
    fn conn_mut(&mut self) -> Result<&mut SqliteConnection, SqlMiddlewareDbError> {
        self.conn.as_mut().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("SQLite transaction already completed".into())
        })
    }

    /// Prepare a statement within this transaction.
    pub fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        Ok(Prepared {
            sql: Arc::new(sql.to_owned()),
        })
    }

    /// Execute a prepared statement as DML within this transaction.
    pub async fn execute_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
        let conn = self.conn_mut()?;
        conn.execute_dml_in_tx(prepared.sql.as_ref(), &converted.0).await
    }

    /// Execute a prepared statement as a query within this transaction.
    pub async fn query_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;
        let conn = self.conn_mut()?;
        conn.execute_select_in_tx(prepared.sql.as_ref(), &converted.0, super::query::build_result_set)
            .await
    }

    /// Execute a batch inside the open transaction.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let conn = self.conn_mut()?;
        conn.execute_batch_in_tx(sql).await
    }

    /// Commit the transaction and return the idle connection.
    pub async fn commit(mut self) -> Result<SqliteConnection, SqlMiddlewareDbError> {
        let mut conn = self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("SQLite transaction already completed".into())
        })?;
        conn.commit().await?;
        Ok(conn)
    }

    /// Roll back the transaction and return the idle connection.
    pub async fn rollback(mut self) -> Result<SqliteConnection, SqlMiddlewareDbError> {
        let mut conn = self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("SQLite transaction already completed".into())
        })?;
        conn.rollback().await?;
        Ok(conn)
    }
}

impl Drop for Tx {
    fn drop(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = conn.rollback().await;
                });
            }
        }
    }
}
