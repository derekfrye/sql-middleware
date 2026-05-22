use std::sync::Arc;

use tokio::sync::Mutex;

use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, CustomDbRow, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::types::StatementCacheMode;

use super::params::{Params as TursoParams, TursoParamsBuf};

/// Handle to a prepared Turso statement owned by a pooled connection.
///
/// This exists to reuse a compiled Turso statement outside an explicit
/// transaction. Turso's client lets us keep a connection-bound prepared handle,
/// so we expose it for non-transactional reuse. Other backends (Postgres)
/// prepare statements on their transaction handles instead of exposing a safe
/// connection-level prepared handle, so we don't mirror this type there.
///
/// Instances can be cloned and reused across awaited calls. Internally, the
/// underlying `turso::Statement` is protected by a `tokio::sync::Mutex` so the
/// compiled statement can be shared safely between tasks while still benefiting
/// from Turso's statement caching.
#[derive(Clone)]
pub struct TursoNonTxPreparedStatement {
    _connection: turso::Connection,
    statement: Arc<Mutex<turso::Statement>>,
    columns: Arc<Vec<String>>,
    sql: Arc<String>,
}

impl std::fmt::Debug for TursoNonTxPreparedStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TursoNonTxPreparedStatement")
            .field("_connection", &"<turso::Connection>")
            .field("statement", &"<turso::Statement>")
            .field("columns", &self.columns)
            .field("sql", &self.sql)
            .finish()
    }
}

impl TursoNonTxPreparedStatement {
    pub(crate) async fn prepare_with_cache_mode(
        connection: turso::Connection,
        sql: &str,
        statement_cache_mode: StatementCacheMode,
    ) -> Result<Self, SqlMiddlewareDbError> {
        let sql_arc = Arc::new(sql.to_owned());
        let statement = match statement_cache_mode {
            StatementCacheMode::Cached => connection.prepare_cached(sql).await,
            StatementCacheMode::Uncached => connection.prepare(sql).await,
        }
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso prepare error: {e}")))?;

        let columns = statement.column_names();

        Ok(Self {
            _connection: connection,
            statement: Arc::new(Mutex::new(statement)),
            columns: Arc::new(columns),
            sql: sql_arc,
        })
    }

    /// Start configuring a prepared SELECT execution.
    #[must_use]
    pub fn select(&self) -> TursoPreparedSelect<'_, '_> {
        TursoPreparedSelect {
            statement: self,
            params: TursoPreparedParams::None,
        }
    }

    /// Start configuring a prepared DML execution.
    #[must_use]
    pub fn execute(&self) -> TursoPreparedExecute<'_, '_> {
        TursoPreparedExecute {
            statement: self,
            params: TursoPreparedParams::None,
        }
    }

    /// Execute the prepared statement as a query and materialise the rows into a [`ResultSet`].
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if parameter conversion fails, the Turso client reports an
    /// execution error, or result decoding cannot be completed.
    pub(crate) async fn query(
        &self,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = convert_params::<TursoParams>(params, ConversionMode::Query)?;
        self.query_driver_params(converted.0).await
    }

    /// Execute the prepared statement as a query using a reusable Turso parameter buffer.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or result decoding cannot be completed.
    pub(crate) async fn query_params(
        &self,
        params: &TursoParamsBuf,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.query_driver_params(params.to_params()).await
    }

    async fn query_driver_params(
        &self,
        params: turso::params::Params,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let rows = {
            let mut stmt = self.statement.lock().await;
            stmt.query(params).await.map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso prepared query error: {e}"))
            })?
        };

        let result = crate::turso::query::build_result_set(rows, Some(self.columns.clone())).await;

        self.reset().await?;
        result
    }

    /// Execute the prepared statement as a query and return the first row, if present.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if parameter conversion, execution, or result decoding
    /// fails.
    pub(crate) async fn query_optional(
        &self,
        params: &[RowValues],
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.query(params).await.map(ResultSet::into_optional)
    }

    /// Execute the prepared statement with a reusable parameter buffer and return the first row,
    /// if present.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if parameter conversion, execution, or result decoding
    /// fails.
    pub(crate) async fn query_optional_params(
        &self,
        params: &TursoParamsBuf,
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.query_params(params)
            .await
            .map(ResultSet::into_optional)
    }

    /// Execute the prepared statement as a query and return the first row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or no row is returned.
    pub(crate) async fn query_one(
        &self,
        params: &[RowValues],
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query(params).await?.into_one()
    }

    /// Execute the prepared statement with a reusable parameter buffer and return the first row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or no row is returned.
    pub(crate) async fn query_one_params(
        &self,
        params: &TursoParamsBuf,
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query_params(params).await?.into_one()
    }

    /// Execute the prepared statement and map the first native Turso row.
    ///
    /// Use this for hot paths that only need one row and can decode directly from
    /// `turso::Row`, avoiding `ResultSet` materialisation.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, no row is returned, or the mapper
    /// fails.
    pub(crate) async fn query_map_one<T, F>(
        &self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.query_map_optional(params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("query returned no rows".into()))
    }

    /// Execute the prepared statement with a reusable parameter buffer and map the first native
    /// Turso row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, no row is returned, or the mapper
    /// fails.
    pub(crate) async fn query_map_one_params<T, F>(
        &self,
        params: &TursoParamsBuf,
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.query_map_optional_params(params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("query returned no rows".into()))
    }

    /// Execute the prepared statement and map the first native Turso row, returning `None` if no
    /// row exists.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or the mapper fails.
    pub(crate) async fn query_map_optional<T, F>(
        &self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        let converted = convert_params::<TursoParams>(params, ConversionMode::Query)?;
        self.query_map_optional_driver_params(converted.0, mapper)
            .await
    }

    /// Execute the prepared statement with a reusable parameter buffer and map the first native
    /// Turso row, returning `None` if no row exists.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or the mapper fails.
    pub(crate) async fn query_map_optional_params<T, F>(
        &self,
        params: &TursoParamsBuf,
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.query_map_optional_driver_params(params.to_params(), mapper)
            .await
    }

    async fn query_map_optional_driver_params<T, F>(
        &self,
        params: turso::params::Params,
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        let rows = {
            let mut stmt = self.statement.lock().await;
            stmt.query(params).await.map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso prepared query error: {e}"))
            })?
        };

        let result = crate::turso::query::query_map_optional(rows, mapper).await;
        self.reset().await?;
        result
    }

    /// Execute the prepared statement as a DML (INSERT/UPDATE/DELETE) returning rows affected.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if parameter conversion fails, Turso returns an execution
    /// error, or the affected-row count cannot be converted into `usize`.
    pub(crate) async fn execute_values(
        &self,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = convert_params::<TursoParams>(params, ConversionMode::Execute)?;
        self.execute_driver_params(converted.0).await
    }

    /// Execute the prepared statement as DML using a reusable Turso parameter buffer.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if Turso returns an execution error, or the affected-row
    /// count cannot be converted into `usize`.
    pub(crate) async fn execute_params(
        &self,
        params: &TursoParamsBuf,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.execute_driver_params(params.to_params()).await
    }

    async fn execute_driver_params(
        &self,
        params: turso::params::Params,
    ) -> Result<usize, SqlMiddlewareDbError> {
        let affected = {
            let mut stmt = self.statement.lock().await;
            let affected = stmt.execute(params).await.map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso prepared execute error: {e}"))
            })?;
            stmt.reset().map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso reset error: {e}"))
            })?;
            affected
        };

        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "Turso affected rows conversion error: {e}"
            ))
        })
    }

    /// Access the raw SQL string of the prepared statement.
    #[must_use]
    pub fn sql(&self) -> &str {
        self.sql.as_str()
    }

    async fn reset(&self) -> Result<(), SqlMiddlewareDbError> {
        let stmt = self.statement.lock().await;
        stmt.reset()
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso reset error: {e}")))
    }
}

/// Builder for executing a prepared Turso DML statement.
pub struct TursoPreparedExecute<'stmt, 'params> {
    statement: &'stmt TursoNonTxPreparedStatement,
    params: TursoPreparedParams<'params>,
}

impl<'stmt, 'params> TursoPreparedExecute<'stmt, 'params> {
    /// Use middleware `RowValues` parameters.
    #[must_use]
    pub fn params<'next>(self, params: &'next [RowValues]) -> TursoPreparedExecute<'stmt, 'next> {
        TursoPreparedExecute {
            statement: self.statement,
            params: TursoPreparedParams::RowValues(params),
        }
    }

    /// Use a reusable Turso parameter buffer.
    #[must_use]
    pub fn params_buf<'next>(
        self,
        params: &'next TursoParamsBuf,
    ) -> TursoPreparedExecute<'stmt, 'next> {
        TursoPreparedExecute {
            statement: self.statement,
            params: TursoPreparedParams::Buffer(params),
        }
    }

    /// Execute the DML statement and return affected rows.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or the row count cannot be converted.
    pub async fn run(self) -> Result<usize, SqlMiddlewareDbError> {
        match self.params {
            TursoPreparedParams::None => self.statement.execute_values(&[]).await,
            TursoPreparedParams::RowValues(params) => self.statement.execute_values(params).await,
            TursoPreparedParams::Buffer(params) => self.statement.execute_params(params).await,
        }
    }
}

enum TursoPreparedParams<'params> {
    None,
    RowValues(&'params [RowValues]),
    Buffer(&'params TursoParamsBuf),
}

/// Builder for executing a prepared Turso SELECT.
pub struct TursoPreparedSelect<'stmt, 'params> {
    statement: &'stmt TursoNonTxPreparedStatement,
    params: TursoPreparedParams<'params>,
}

impl<'stmt, 'params> TursoPreparedSelect<'stmt, 'params> {
    /// Use middleware `RowValues` parameters.
    #[must_use]
    pub fn params<'next>(self, params: &'next [RowValues]) -> TursoPreparedSelect<'stmt, 'next> {
        TursoPreparedSelect {
            statement: self.statement,
            params: TursoPreparedParams::RowValues(params),
        }
    }

    /// Use a reusable Turso parameter buffer.
    #[must_use]
    pub fn params_buf<'next>(
        self,
        params: &'next TursoParamsBuf,
    ) -> TursoPreparedSelect<'stmt, 'next> {
        TursoPreparedSelect {
            statement: self.statement,
            params: TursoPreparedParams::Buffer(params),
        }
    }

    /// Execute and return all rows as a `ResultSet`.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or result decoding cannot be completed.
    pub async fn all(self) -> Result<ResultSet, SqlMiddlewareDbError> {
        match self.params {
            TursoPreparedParams::None => self.statement.query(&[]).await,
            TursoPreparedParams::RowValues(params) => self.statement.query(params).await,
            TursoPreparedParams::Buffer(params) => self.statement.query_params(params).await,
        }
    }

    /// Execute and return the first row, if present.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or result decoding fails.
    pub async fn optional(self) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        match self.params {
            TursoPreparedParams::None => self.statement.query_optional(&[]).await,
            TursoPreparedParams::RowValues(params) => self.statement.query_optional(params).await,
            TursoPreparedParams::Buffer(params) => {
                self.statement.query_optional_params(params).await
            }
        }
    }

    /// Execute and return exactly one row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or no row is returned.
    pub async fn one(self) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        match self.params {
            TursoPreparedParams::None => self.statement.query_one(&[]).await,
            TursoPreparedParams::RowValues(params) => self.statement.query_one(params).await,
            TursoPreparedParams::Buffer(params) => self.statement.query_one_params(params).await,
        }
    }

    /// Execute and map exactly one native Turso row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, no row is returned, or the mapper fails.
    pub async fn map_one<T, F>(self, mapper: F) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        match self.params {
            TursoPreparedParams::None => self.statement.query_map_one(&[], mapper).await,
            TursoPreparedParams::RowValues(params) => {
                self.statement.query_map_one(params, mapper).await
            }
            TursoPreparedParams::Buffer(params) => {
                self.statement.query_map_one_params(params, mapper).await
            }
        }
    }

    /// Execute and map the first native Turso row, if present.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or the mapper fails.
    pub async fn map_optional<T, F>(self, mapper: F) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        match self.params {
            TursoPreparedParams::None => self.statement.query_map_optional(&[], mapper).await,
            TursoPreparedParams::RowValues(params) => {
                self.statement.query_map_optional(params, mapper).await
            }
            TursoPreparedParams::Buffer(params) => {
                self.statement
                    .query_map_optional_params(params, mapper)
                    .await
            }
        }
    }
}
