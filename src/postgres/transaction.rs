use deadpool_postgres::{Object, Transaction as PgTransaction};
use tokio_postgres::Statement;

use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};

use super::{Params, build_result_set};

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
pub async fn begin_transaction(conn: &mut Object) -> Result<Tx<'_>, SqlMiddlewareDbError> {
    let tx = conn.transaction().await?;
    Ok(Tx { tx })
}

impl<'a> Tx<'a> {
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
    pub async fn commit(self) -> Result<(), SqlMiddlewareDbError> {
        self.tx.commit().await?;
        Ok(())
    }

    /// Roll back the transaction.
    ///
    /// # Errors
    /// Returns an error if rollback fails.
    pub async fn rollback(self) -> Result<(), SqlMiddlewareDbError> {
        self.tx.rollback().await?;
        Ok(())
    }
}
