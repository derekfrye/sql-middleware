use tiberius::Query;

use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use crate::tx_outcome::TxOutcome;

use super::config::MssqlClient;
use super::query::{build_result_set, convert_affected_rows};

/// Lightweight transaction wrapper for SQL Server.
///
/// Dropping a `Tx` without calling [`commit`](Tx::commit) or [`rollback`](Tx::rollback)
/// leaves the connection mid-transaction. Always finish the transaction explicitly.
pub struct Tx<'a> {
    client: &'a mut MssqlClient,
    open: bool,
}

/// Prepared statement wrapper for SQL Server.
///
/// This is a minimal wrapper that stores the SQL text; execution is delegated to
/// the shared `bind_query_params` + `build_result_set` helpers.
pub struct Prepared {
    sql: String,
}

/// Begin a new transaction on the provided SQL Server connection.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if issuing the BEGIN statement fails.
pub async fn begin_transaction(client: &mut MssqlClient) -> Result<Tx<'_>, SqlMiddlewareDbError> {
    Query::new("BEGIN TRANSACTION")
        .execute(client)
        .await
        .map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL begin transaction error: {e}"))
        })?;

    Ok(Tx { client, open: true })
}

impl Tx<'_> {
    /// Prepare a SQL statement tied to this transaction.
    ///
    /// # Errors
    /// Returns an error if preparing the statement fails (validation is done on first use).
    pub fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        Ok(Prepared {
            sql: sql.to_string(),
        })
    }

    /// Execute a batch of SQL statements inside the transaction.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        Query::new(sql).execute(self.client).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL tx execute_batch error: {e}"))
        })?;
        Ok(())
    }

    /// Execute a DML statement inside the transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails or the affected row count cannot be converted.
    pub async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let query_builder = super::query::bind_query_params(query, params);
        let exec_result = query_builder.execute(self.client).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL tx execute error: {e}"))
        })?;

        let rows_affected: u64 = exec_result.rows_affected().iter().sum();
        convert_affected_rows(rows_affected)
    }

    /// Execute a prepared DML statement and return affected rows.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails or the affected row count cannot be converted.
    pub async fn execute_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let query_builder = super::query::bind_query_params(&prepared.sql, params);
        let exec_result = query_builder.execute(self.client).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL tx execute error: {e}"))
        })?;

        let rows_affected: u64 = exec_result.rows_affected().iter().sum();
        convert_affected_rows(rows_affected)
    }

    /// Execute a prepared SELECT and return a `ResultSet`.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if execution or result construction fails.
    pub async fn query_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        build_result_set(self.client, &prepared.sql, params).await
    }

    /// Execute a SELECT inside the transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution or result construction fails.
    pub async fn query(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        build_result_set(self.client, query, params).await
    }

    /// Commit the transaction.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if commit fails.
    pub async fn commit(mut self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        if self.open {
            Query::new("COMMIT TRANSACTION")
                .execute(self.client)
                .await
                .map_err(|e| {
                    SqlMiddlewareDbError::ExecutionError(format!("MSSQL commit error: {e}"))
                })?;
            self.open = false;
        }
        Ok(TxOutcome::without_restored_connection())
    }

    /// Roll back the transaction.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if rollback fails.
    pub async fn rollback(mut self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        if self.open {
            Query::new("ROLLBACK TRANSACTION")
                .execute(self.client)
                .await
                .map_err(|e| {
                    SqlMiddlewareDbError::ExecutionError(format!("MSSQL rollback error: {e}"))
                })?;
            self.open = false;
        }
        Ok(TxOutcome::without_restored_connection())
    }
}
