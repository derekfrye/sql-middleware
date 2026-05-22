use std::sync::Arc;

use tokio::sync::Mutex;

use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, CustomDbRow, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::types::StatementCacheMode;

use super::params::Params as TursoParams;

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

    /// Execute the prepared statement as a query and materialise the rows into a [`ResultSet`].
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if parameter conversion fails, the Turso client reports an
    /// execution error, or result decoding cannot be completed.
    pub async fn query(&self, params: &[RowValues]) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = convert_params::<TursoParams>(params, ConversionMode::Query)?;

        let rows = {
            let mut stmt = self.statement.lock().await;
            stmt.query(converted.0).await.map_err(|e| {
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
    pub async fn query_optional(
        &self,
        params: &[RowValues],
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.query(params).await.map(ResultSet::into_optional)
    }

    /// Execute the prepared statement as a query and return the first row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or no row is returned.
    pub async fn query_one(
        &self,
        params: &[RowValues],
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query(params).await?.into_one()
    }

    /// Execute the prepared statement and map the first native Turso row.
    ///
    /// Use this for hot paths that only need one row and can decode directly from
    /// `turso::Row`, avoiding `ResultSet` materialisation.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, no row is returned, or the mapper
    /// fails.
    pub async fn query_map_one<T, F>(
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

    /// Execute the prepared statement and map the first native Turso row, returning `None` if no
    /// row exists.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or the mapper fails.
    pub async fn query_map_optional<T, F>(
        &self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&turso::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        let converted = convert_params::<TursoParams>(params, ConversionMode::Query)?;

        let rows = {
            let mut stmt = self.statement.lock().await;
            stmt.query(converted.0).await.map_err(|e| {
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
    pub async fn execute(&self, params: &[RowValues]) -> Result<usize, SqlMiddlewareDbError> {
        let converted = convert_params::<TursoParams>(params, ConversionMode::Execute)?;

        let affected = {
            let mut stmt = self.statement.lock().await;
            let affected = stmt.execute(converted.0).await.map_err(|e| {
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
