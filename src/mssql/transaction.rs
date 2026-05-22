use tiberius::Query;

use crate::middleware::{CustomDbRow, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::tx_outcome::TxOutcome;

use super::config::MssqlClient;
use super::query::{build_result_set, convert_affected_rows, query_map_optional};

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
    client
        .simple_query("BEGIN TRANSACTION")
        .await
        .map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL begin transaction error: {e}"))
        })?;

    Ok(Tx { client, open: true })
}

impl<'conn> Tx<'conn> {
    /// Prepare a SQL statement tied to this transaction.
    ///
    /// # Errors
    /// Returns an error if preparing the statement fails (validation is done on first use).
    pub fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        Ok(Prepared {
            sql: sql.to_string(),
        })
    }

    /// Start configuring a prepared SELECT execution.
    #[must_use]
    pub fn select<'tx, 'prepared>(
        &'tx mut self,
        prepared: &'prepared Prepared,
    ) -> PreparedSelect<'tx, 'prepared, 'static, 'conn> {
        PreparedSelect {
            tx: self,
            prepared,
            params: &[],
        }
    }

    /// Start configuring a prepared DML execution.
    #[must_use]
    pub fn execute<'tx, 'prepared>(
        &'tx mut self,
        prepared: &'prepared Prepared,
    ) -> PreparedExecute<'tx, 'prepared, 'static, 'conn> {
        PreparedExecute {
            tx: self,
            prepared,
            params: &[],
        }
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
    pub(crate) async fn execute_prepared(
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
    pub(crate) async fn query_prepared(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        build_result_set(self.client, &prepared.sql, params).await
    }

    /// Execute a prepared SELECT and return the first row, if present.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution or result construction fails.
    pub(crate) async fn query_prepared_optional(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.query_prepared(prepared, params)
            .await
            .map(ResultSet::into_optional)
    }

    /// Execute a prepared SELECT and return the first row.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution fails or no row is returned.
    pub(crate) async fn query_prepared_one(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query_prepared(prepared, params).await?.into_one()
    }

    /// Execute a prepared SELECT and map the first native SQL Server row.
    ///
    /// Use this for hot paths that only need one row and can decode directly from
    /// `tiberius::Row`, avoiding `ResultSet` materialisation.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution fails, no row is returned, or the mapper fails.
    pub(crate) async fn query_prepared_map_one<T, F>(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.query_prepared_map_optional(prepared, params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("query returned no rows".into()))
    }

    /// Execute a prepared SELECT and map the first native SQL Server row, returning `None` if no
    /// row exists.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution or the mapper fails.
    pub(crate) async fn query_prepared_map_optional<T, F>(
        &mut self,
        prepared: &Prepared,
        params: &[RowValues],
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        query_map_optional(self.client, &prepared.sql, params, mapper).await
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
            self.client
                .simple_query("COMMIT TRANSACTION")
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
            self.client
                .simple_query("ROLLBACK TRANSACTION")
                .await
                .map_err(|e| {
                    SqlMiddlewareDbError::ExecutionError(format!("MSSQL rollback error: {e}"))
                })?;
            self.open = false;
        }
        Ok(TxOutcome::without_restored_connection())
    }
}

/// Builder for executing a prepared SQL Server DML statement inside a transaction.
pub struct PreparedExecute<'tx, 'prepared, 'params, 'conn> {
    tx: &'tx mut Tx<'conn>,
    prepared: &'prepared Prepared,
    params: &'params [RowValues],
}

impl<'tx, 'prepared, 'params, 'conn> PreparedExecute<'tx, 'prepared, 'params, 'conn> {
    /// Use middleware `RowValues` parameters.
    #[must_use]
    pub fn params<'next>(
        self,
        params: &'next [RowValues],
    ) -> PreparedExecute<'tx, 'prepared, 'next, 'conn> {
        PreparedExecute {
            tx: self.tx,
            prepared: self.prepared,
            params,
        }
    }

    /// Execute the DML statement and return affected rows.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution fails or the affected row count cannot be converted.
    pub async fn run(self) -> Result<usize, SqlMiddlewareDbError> {
        self.tx.execute_prepared(self.prepared, self.params).await
    }
}

/// Builder for executing a prepared SQL Server SELECT inside a transaction.
pub struct PreparedSelect<'tx, 'prepared, 'params, 'conn> {
    tx: &'tx mut Tx<'conn>,
    prepared: &'prepared Prepared,
    params: &'params [RowValues],
}

impl<'tx, 'prepared, 'params, 'conn> PreparedSelect<'tx, 'prepared, 'params, 'conn> {
    /// Use middleware `RowValues` parameters.
    #[must_use]
    pub fn params<'next>(
        self,
        params: &'next [RowValues],
    ) -> PreparedSelect<'tx, 'prepared, 'next, 'conn> {
        PreparedSelect {
            tx: self.tx,
            prepared: self.prepared,
            params,
        }
    }

    /// Execute and return all rows as a `ResultSet`.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution or result construction fails.
    pub async fn all(self) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.tx.query_prepared(self.prepared, self.params).await
    }

    /// Execute and return the first row, if present.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution or result construction fails.
    pub async fn optional(self) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.tx
            .query_prepared_optional(self.prepared, self.params)
            .await
    }

    /// Execute and return exactly one row.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution fails or no row is returned.
    pub async fn one(self) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.tx.query_prepared_one(self.prepared, self.params).await
    }

    /// Execute and map exactly one native SQL Server row.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution fails, no row is returned, or the mapper fails.
    pub async fn map_one<T, F>(self, mapper: F) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.tx
            .query_prepared_map_one(self.prepared, self.params, mapper)
            .await
    }

    /// Execute and map the first native SQL Server row, if present.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if execution or the mapper fails.
    pub async fn map_optional<T, F>(self, mapper: F) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.tx
            .query_prepared_map_optional(self.prepared, self.params, mapper)
            .await
    }
}
