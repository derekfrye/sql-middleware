use std::future::Future;
use std::sync::Arc;
use std::pin::Pin;

use crate::middleware::{ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::turso::params::Params as TursoParams;

/// Lightweight transaction wrapper for Turso.
///
/// This wrapper issues explicit `BEGIN`, `COMMIT`, and `ROLLBACK` statements on the
/// provided `turso::Connection` and exposes helpers to run queries within that
/// transaction. It does not depend on a dedicated `turso::Transaction` type, keeping
/// the API stable and avoiding additional type leakage.
pub struct Tx<'a> {
    pub(crate) conn: &'a turso::Connection,
}

impl<'a> Tx<'a> {
    /// Execute a batch of SQL statements within the transaction.
    pub async fn execute_batch(&self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn
            .execute_batch(sql)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso tx execute_batch error: {e}")))
    }

    /// Execute a parameterized DML statement and return affected rows.
    pub async fn execute_dml(
        &self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
        let affected = self
            .conn
            .execute(query, converted.0)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso tx execute error: {e}")))?;
        usize::try_from(affected).map_err(|e| SqlMiddlewareDbError::ExecutionError(format!(
            "Turso affected rows conversion error: {e}"
        )))
    }

    /// Execute a parameterized SELECT and return a `ResultSet`.
    pub async fn execute_select(
        &self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;

        // Prepare to fetch column names, then run using same statement to avoid double-prepare.
        let mut stmt = self
            .conn
            .prepare(query)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso tx prepare error: {e}")))?;

        let cols = stmt
            .columns()
            .into_iter()
            .map(|c| c.name().to_string())
            .collect::<Vec<_>>();
        let cols_arc = Arc::new(cols);

        let rows = stmt
            .query(converted.0)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso tx query error: {e}")))?;

        crate::turso::query::build_result_set(rows, Some(cols_arc)).await
    }

    /// Commit the transaction.
    pub async fn commit(&self) -> Result<(), SqlMiddlewareDbError> {
        self.conn
            .execute_batch("COMMIT")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso commit error: {e}")))
    }

    /// Roll back the transaction.
    pub async fn rollback(&self) -> Result<(), SqlMiddlewareDbError> {
        self.conn
            .execute_batch("ROLLBACK")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso rollback error: {e}")))
    }
}

/// Begin a new transaction for the given connection.
pub async fn begin_transaction<'a>(conn: &'a turso::Connection) -> Result<Tx<'a>, SqlMiddlewareDbError> {
    conn
        .execute_batch("BEGIN")
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso begin transaction error: {e}")))?;
    Ok(Tx { conn })
}

/// Run a closure inside a transaction, committing on success, rolling back on error.
pub async fn with_transaction<F, T>(
    conn: &turso::Connection,
    f: F,
) -> Result<T, SqlMiddlewareDbError>
where
    F: for<'a> FnOnce(&'a Tx<'a>) -> Pin<Box<dyn Future<Output = Result<T, SqlMiddlewareDbError>> + 'a>>,
{
    let mut tx = begin_transaction(conn).await?;
    let res = f(&tx).await;
    match res {
        Ok(val) => {
            tx.commit().await?;
            Ok(val)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}
