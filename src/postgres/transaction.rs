use std::ops::DerefMut;

use tokio_postgres::{Client, Statement, Transaction as PgTransaction};

use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, CustomDbRow, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::tx_outcome::TxOutcome;

use super::{Params, build_result_set};
use crate::postgres::query::{build_result_set_from_rows, convert_affected_rows};

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

impl<'conn> Tx<'conn> {
    /// Prepare a SQL statement tied to this transaction.
    ///
    /// # Errors
    /// Returns an error if the prepare call fails.
    pub async fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        let stmt = self.tx.prepare(sql).await?;
        Ok(Prepared { stmt })
    }

    /// Start configuring a prepared SELECT execution.
    #[must_use]
    pub fn select<'tx, 'prepared>(
        &'tx self,
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
        &'tx self,
        prepared: &'prepared Prepared,
    ) -> PreparedExecute<'tx, 'prepared, 'static, 'conn> {
        PreparedExecute {
            tx: self,
            prepared,
            params: &[],
        }
    }

    /// Execute a parameterized DML statement and return the affected row count.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or row-count conversion fails.
    pub(crate) async fn execute_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = convert_params::<Params>(params, ConversionMode::Execute)?;

        let rows = self.tx.execute(&prepared.stmt, converted.as_refs()).await?;

        convert_affected_rows(rows, "Invalid rows affected count")
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
        let converted = convert_params::<Params>(params, ConversionMode::Execute)?;
        let rows = self.tx.execute(query, converted.as_refs()).await?;
        convert_affected_rows(rows, "Invalid rows affected count")
    }

    /// Execute a parameterized SELECT and return a `ResultSet`.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result building fails.
    pub(crate) async fn query_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = convert_params::<Params>(params, ConversionMode::Query)?;
        build_result_set(&prepared.stmt, converted.as_refs(), &self.tx).await
    }

    /// Execute a prepared SELECT and return the first row, if present.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result building fails.
    pub(crate) async fn query_prepared_optional(
        &self,
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
    /// Returns an error if execution fails or no row is returned.
    pub(crate) async fn query_prepared_one(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query_prepared(prepared, params).await?.into_one()
    }

    /// Execute a prepared SELECT and map the first native Postgres row.
    ///
    /// Use this for hot paths that only need one row and can decode directly from
    /// `tokio_postgres::Row`, avoiding `ResultSet` materialisation.
    ///
    /// # Errors
    /// Returns an error if execution fails, no row is returned, or the mapper fails.
    pub(crate) async fn query_prepared_map_one<T, F>(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&tokio_postgres::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.query_prepared_map_optional(prepared, params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("query returned no rows".into()))
    }

    /// Execute a prepared SELECT and map the first native Postgres row, returning `None` if no row
    /// exists.
    ///
    /// # Errors
    /// Returns an error if execution or the mapper fails.
    pub(crate) async fn query_prepared_map_optional<T, F>(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&tokio_postgres::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        let converted = convert_params::<Params>(params, ConversionMode::Query)?;
        let row = self
            .tx
            .query_opt(&prepared.stmt, converted.as_refs())
            .await?;
        row.as_ref().map(mapper).transpose()
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
        let converted = convert_params::<Params>(params, ConversionMode::Query)?;
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

/// Builder for executing a prepared Postgres DML statement inside a transaction.
pub struct PreparedExecute<'tx, 'prepared, 'params, 'conn> {
    tx: &'tx Tx<'conn>,
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
    /// Returns an error if parameter conversion, execution, or row-count conversion fails.
    pub async fn run(self) -> Result<usize, SqlMiddlewareDbError> {
        self.tx.execute_prepared(self.prepared, self.params).await
    }
}

/// Builder for executing a prepared Postgres SELECT inside a transaction.
pub struct PreparedSelect<'tx, 'prepared, 'params, 'conn> {
    tx: &'tx Tx<'conn>,
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
    /// Returns an error if parameter conversion, execution, or result building fails.
    pub async fn all(self) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.tx.query_prepared(self.prepared, self.params).await
    }

    /// Execute and return the first row, if present.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result building fails.
    pub async fn optional(self) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.tx
            .query_prepared_optional(self.prepared, self.params)
            .await
    }

    /// Execute and return exactly one row.
    ///
    /// # Errors
    /// Returns an error if execution fails or no row is returned.
    pub async fn one(self) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.tx.query_prepared_one(self.prepared, self.params).await
    }

    /// Execute and map exactly one native Postgres row.
    ///
    /// # Errors
    /// Returns an error if execution fails, no row is returned, or the mapper fails.
    pub async fn map_one<T, F>(self, mapper: F) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&tokio_postgres::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.tx
            .query_prepared_map_one(self.prepared, self.params, mapper)
            .await
    }

    /// Execute and map the first native Postgres row, if present.
    ///
    /// # Errors
    /// Returns an error if execution or the mapper fails.
    pub async fn map_optional<T, F>(self, mapper: F) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&tokio_postgres::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.tx
            .query_prepared_map_optional(self.prepared, self.params, mapper)
            .await
    }
}
