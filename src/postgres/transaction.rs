use std::ops::DerefMut;

use tokio_postgres::{Client, Statement, Transaction as PgTransaction};

use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};
use crate::tx_outcome::TxOutcome;

use super::{Params, build_result_set};
use crate::postgres::query::build_result_set_from_rows;

/// Lightweight transaction wrapper for Postgres.
pub struct Tx<'a> {
    tx: PgTransaction<'a>,
}

/// Prepared statement wrapper for Postgres.
pub struct Prepared {
    stmt: Statement,
}

/// Begin a new transaction on the provided Postgres connection.
///
/// # Errors
/// Returns an error if creating the transaction fails.
pub async fn begin_transaction<C>(conn: &mut C) -> Result<Tx<'_>, SqlMiddlewareDbError>
where
    C: DerefMut<Target = Client>,
{
    let tx = conn.deref_mut().transaction().await?;
    Ok(Tx { tx })
}

impl Tx<'_> {
    /// Prepare a SQL statement tied to this transaction.
    ///
    /// # Errors
    /// Returns an error if the prepare call fails.
    pub async fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        let stmt = self.tx.prepare(sql).await?;
        Ok(Prepared { stmt })
    }

    /// Execute a parameterized DML statement and return the affected row count.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or row-count conversion fails.
    pub async fn execute_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;

        let rows = self.tx.execute(&prepared.stmt, converted.as_refs()).await?;

        usize::try_from(rows).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Invalid rows affected count: {e}"))
        })
    }

    /// Execute a parameterized DML statement without preparing and return affected rows.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or row-count conversion fails.
    pub async fn execute_dml(
        &self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
        let rows = self.tx.execute(query, converted.as_refs()).await?;
        usize::try_from(rows).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Invalid rows affected count: {e}"))
        })
    }

    /// Execute a parameterized SELECT and return a `ResultSet`.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result building fails.
    pub async fn query_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;
        build_result_set(&prepared.stmt, converted.as_refs(), &self.tx).await
    }

    /// Execute a parameterized SELECT without preparing and return a `ResultSet`.
    ///
    /// # Errors
    /// Returns an error if parameter conversion or query execution fails.
    pub async fn query(
        &self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;
        let rows = self.tx.query(query, converted.as_refs()).await?;
        build_result_set_from_rows(&rows)
    }

    /// Execute a batch of SQL statements inside the transaction.
    ///
    /// # Errors
    /// Returns an error if execution fails.
    pub async fn execute_batch(&self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.tx.batch_execute(sql).await?;
        Ok(())
    }

    /// Commit the transaction.
    ///
    /// # Errors
    /// Returns an error if commit fails.
    pub async fn commit(self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        self.tx.commit().await?;
        Ok(TxOutcome::without_restored_connection())
    }

    /// Roll back the transaction.
    ///
    /// # Errors
    /// Returns an error if rollback fails.
    pub async fn rollback(self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        self.tx.rollback().await?;
        Ok(TxOutcome::without_restored_connection())
    }
}
