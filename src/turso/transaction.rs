use std::sync::Arc;

use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};
use crate::query_utils::extract_column_names;
use crate::turso::params::Params as TursoParams;
use crate::tx_outcome::TxOutcome;

/// Lightweight transaction wrapper for Turso.
///
/// Wraps a `turso::transaction::Transaction` to keep the public API stable while
/// benefiting from Turso's transaction-scoped helpers (including prepare).
pub struct Tx<'a> {
    pub(crate) tx: turso::transaction::Transaction<'a>,
}

/// Prepared statement wrapper for Turso.
/// Lifetime is tied to the underlying connection held by Tx.
pub struct Prepared {
    stmt: turso::Statement,
    cols: Arc<Vec<String>>, // cached column names for fast ResultSet builds
}

impl Tx<'_> {
    /// Prepare a SQL statement tied to this transaction's connection.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when the underlying Turso prepare call fails.
    pub async fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        let stmt = self.tx.prepare(sql).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso prepare error: {e}"))
        })?;

        let cols = extract_column_names(stmt.columns().iter(), |col| col.name());

        Ok(Prepared {
            stmt,
            cols: Arc::new(cols),
        })
    }

    /// Execute a batch of SQL statements within the transaction.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when the Turso batch execution fails.
    pub async fn execute_batch(&self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.tx.execute_batch(sql).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso tx execute_batch error: {e}"))
        })
    }

    /// Execute a parameterized DML statement and return affected rows.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when executing the statement fails or the affected row
    /// count cannot be converted to `usize`.
    pub async fn execute_dml(
        &self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
        let affected = self.tx.execute(query, converted.0).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso tx execute error: {e}"))
        })?;
        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "Turso affected rows conversion error: {e}"
            ))
        })
    }

    /// Execute a prepared DML and return affected row count.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when executing the prepared statement fails or the
    /// affected row count cannot be converted to `usize`.
    pub async fn execute_prepared(
        &self,
        prepared: &mut Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
        let affected = prepared.stmt.execute(converted.0).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso tx execute(prepared) error: {e}"))
        })?;
        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "Turso affected rows conversion error: {e}"
            ))
        })
    }

    /// Execute a parameterized SELECT and return a `ResultSet`.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when preparing/executing the statement or building the
    /// `ResultSet` fails.
    pub async fn execute_select(
        &self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted =
            <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;

        // Prepare to fetch column names, then run using same statement to avoid double-prepare.
        let mut stmt = self.tx.prepare(query).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso tx prepare error: {e}"))
        })?;

        let cols = extract_column_names(stmt.columns().iter(), |col| col.name());
        let cols_arc = Arc::new(cols);

        let rows = stmt.query(converted.0).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso tx query error: {e}"))
        })?;

        crate::turso::query::build_result_set(rows, Some(cols_arc)).await
    }

    /// Execute a prepared SELECT and return a `ResultSet`.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when running the prepared statement or building the
    /// `ResultSet` fails.
    pub async fn query_prepared(
        &self,
        prepared: &mut Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted =
            <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;
        let rows = prepared.stmt.query(converted.0).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso tx query(prepared) error: {e}"))
        })?;
        crate::turso::query::build_result_set(rows, Some(prepared.cols.clone())).await
    }

    /// Commit the transaction.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when issuing the COMMIT statement fails.
    pub async fn commit(self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        self.tx
            .commit()
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso commit error: {e}")))
            .map(|()| TxOutcome::without_restored_connection())
    }

    /// Roll back the transaction.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` when issuing the ROLLBACK statement fails.
    pub async fn rollback(self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        self.tx
            .rollback()
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso rollback error: {e}")))
            .map(|()| TxOutcome::without_restored_connection())
    }
}

/// Begin a new transaction for the given connection.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError` when issuing the BEGIN statement fails.
pub async fn begin_transaction(
    conn: &mut turso::Connection,
) -> Result<Tx<'_>, SqlMiddlewareDbError> {
    let tx = conn.transaction().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Turso begin transaction error: {e}"))
    })?;
    Ok(Tx { tx })
}
