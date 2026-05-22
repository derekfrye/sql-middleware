use std::sync::Arc;

use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, CustomDbRow, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::types::StatementCacheMode;

use super::connection::{SqliteConnection, run_blocking};
use super::params::{Params, SqliteParamsBuf};
use super::query::sqlite_extract_value_sync;

/// Handle to a prepared `SQLite` statement tied to a connection.
pub struct SqlitePreparedStatement<'conn> {
    connection: &'conn mut SqliteConnection,
    query: Arc<String>,
    statement_cache_mode: StatementCacheMode,
}

impl<'conn> SqlitePreparedStatement<'conn> {
    pub(crate) fn new(
        connection: &'conn mut SqliteConnection,
        query: Arc<String>,
        statement_cache_mode: StatementCacheMode,
    ) -> Self {
        Self {
            connection,
            query,
            statement_cache_mode,
        }
    }

    /// Execute the prepared statement as a query and materialise the rows into a [`ResultSet`].
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or result conversion encounters an issue.
    pub async fn query(&mut self, params: &[RowValues]) -> Result<ResultSet, SqlMiddlewareDbError> {
        let params_owned = convert_params::<Params>(params, ConversionMode::Query)?.0;
        self.connection
            .execute_select(
                self.query.as_ref(),
                &params_owned,
                super::query::build_result_set,
                self.statement_cache_mode,
            )
            .await
    }

    /// Execute the prepared statement using a reusable SQLite parameter buffer.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or result conversion encounters an issue.
    pub async fn query_params(
        &mut self,
        params: &SqliteParamsBuf,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.connection
            .execute_select(
                self.query.as_ref(),
                params.as_values(),
                super::query::build_result_set,
                self.statement_cache_mode,
            )
            .await
    }

    /// Execute the prepared statement and return the first row, if present.
    ///
    /// This avoids building a full [`ResultSet`] when callers only need one row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or row conversion fails.
    pub async fn query_optional(
        &mut self,
        params: &[RowValues],
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        let params_owned = convert_params::<Params>(params, ConversionMode::Query)?.0;
        self.query_optional_values(params_owned).await
    }

    /// Execute the prepared statement with a reusable parameter buffer and return the first row,
    /// if present.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or row conversion fails.
    pub async fn query_optional_params(
        &mut self,
        params: &SqliteParamsBuf,
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.query_optional_values(params.as_values().to_vec())
            .await
    }

    async fn query_optional_values(
        &mut self,
        params_owned: Vec<rusqlite::types::Value>,
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        let sql = Arc::clone(&self.query);
        let statement_cache_mode = self.statement_cache_mode;
        run_blocking(
            self.connection.conn_handle(),
            move |guard| match statement_cache_mode {
                StatementCacheMode::Cached => {
                    let mut stmt = guard
                        .prepare_cached(sql.as_ref())
                        .map_err(SqlMiddlewareDbError::SqliteError)?;
                    query_optional_with_statement(&mut stmt, &params_owned)
                }
                StatementCacheMode::Uncached => {
                    let mut stmt = guard
                        .prepare(sql.as_ref())
                        .map_err(SqlMiddlewareDbError::SqliteError)?;
                    query_optional_with_statement(&mut stmt, &params_owned)
                }
            },
        )
        .await
    }

    /// Execute the prepared statement and return the first row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, row conversion fails, or no row is
    /// returned.
    pub async fn query_one(
        &mut self,
        params: &[RowValues],
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query_optional(params)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::SqliteError(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Execute the prepared statement with a reusable parameter buffer and return the first row.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, row conversion fails, or no row is
    /// returned.
    pub async fn query_one_params(
        &mut self,
        params: &SqliteParamsBuf,
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query_optional_params(params)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::SqliteError(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Execute the prepared statement and map the first row inside the SQLite worker.
    ///
    /// Use this for hot paths that only need one row and can decode directly from
    /// `rusqlite::Row`, avoiding `ResultSet` materialisation.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, the mapper fails, or no row is
    /// returned.
    pub async fn query_map_one<T, F>(
        &mut self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, SqlMiddlewareDbError> + Send + 'static,
    {
        self.query_map_optional(params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::SqliteError(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Execute the prepared statement with a reusable parameter buffer and map the first row
    /// inside the SQLite worker.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails, the mapper fails, or no row is
    /// returned.
    pub async fn query_map_one_params<T, F>(
        &mut self,
        params: &SqliteParamsBuf,
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, SqlMiddlewareDbError> + Send + 'static,
    {
        self.query_map_optional_params(params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::SqliteError(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Execute the prepared statement and map the first row inside the SQLite worker, returning
    /// `None` when the query has no rows.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or the mapper fails.
    pub async fn query_map_optional<T, F>(
        &mut self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, SqlMiddlewareDbError> + Send + 'static,
    {
        let params_owned = convert_params::<Params>(params, ConversionMode::Query)?.0;
        self.query_map_optional_values(params_owned, mapper).await
    }

    /// Execute the prepared statement with a reusable parameter buffer and map the first row
    /// inside the SQLite worker, returning `None` when the query has no rows.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution or the mapper fails.
    pub async fn query_map_optional_params<T, F>(
        &mut self,
        params: &SqliteParamsBuf,
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, SqlMiddlewareDbError> + Send + 'static,
    {
        self.query_map_optional_values(params.as_values().to_vec(), mapper)
            .await
    }

    async fn query_map_optional_values<T, F>(
        &mut self,
        params_owned: Vec<rusqlite::types::Value>,
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T, SqlMiddlewareDbError> + Send + 'static,
    {
        let sql = Arc::clone(&self.query);
        let statement_cache_mode = self.statement_cache_mode;
        run_blocking(
            self.connection.conn_handle(),
            move |guard| match statement_cache_mode {
                StatementCacheMode::Cached => {
                    let mut stmt = guard
                        .prepare_cached(sql.as_ref())
                        .map_err(SqlMiddlewareDbError::SqliteError)?;
                    query_map_optional_with_statement(&mut stmt, &params_owned, mapper)
                }
                StatementCacheMode::Uncached => {
                    let mut stmt = guard
                        .prepare(sql.as_ref())
                        .map_err(SqlMiddlewareDbError::SqliteError)?;
                    query_map_optional_with_statement(&mut stmt, &params_owned, mapper)
                }
            },
        )
        .await
    }

    /// Execute the prepared statement as a DML (INSERT/UPDATE/DELETE) returning rows affected.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or if the result cannot be converted into the expected row count.
    pub async fn execute(&mut self, params: &[RowValues]) -> Result<usize, SqlMiddlewareDbError> {
        let params_owned = convert_params::<Params>(params, ConversionMode::Execute)?.0;
        self.connection
            .execute_dml(
                self.query.as_ref(),
                &params_owned,
                self.statement_cache_mode,
            )
            .await
    }

    /// Execute the prepared statement as DML using a reusable SQLite parameter buffer.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or if the result cannot be converted into the expected row count.
    pub async fn execute_params(
        &mut self,
        params: &SqliteParamsBuf,
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.connection
            .execute_dml(
                self.query.as_ref(),
                params.as_values(),
                self.statement_cache_mode,
            )
            .await
    }

    /// Access the raw SQL string of the prepared statement.
    #[must_use]
    pub fn sql(&self) -> &str {
        self.query.as_str()
    }
}

fn query_optional_with_statement(
    stmt: &mut rusqlite::Statement<'_>,
    params: &[rusqlite::types::Value],
) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
    let column_names = Arc::new(
        stmt.column_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
    );
    let col_count = column_names.len();
    let param_refs = params
        .iter()
        .map(|value| value as &dyn rusqlite::ToSql)
        .collect::<Vec<_>>();
    let mut rows = stmt.query(&param_refs[..])?;

    rows.next()?
        .map(|row| row_to_custom_db_row(row, Arc::clone(&column_names), col_count))
        .transpose()
}

fn query_map_optional_with_statement<T, F>(
    stmt: &mut rusqlite::Statement<'_>,
    params: &[rusqlite::types::Value],
    mapper: F,
) -> Result<Option<T>, SqlMiddlewareDbError>
where
    F: FnOnce(&rusqlite::Row<'_>) -> Result<T, SqlMiddlewareDbError>,
{
    let param_refs = params
        .iter()
        .map(|value| value as &dyn rusqlite::ToSql)
        .collect::<Vec<_>>();
    let mut rows = stmt.query(&param_refs[..])?;

    rows.next()?.map(mapper).transpose()
}

fn row_to_custom_db_row(
    row: &rusqlite::Row<'_>,
    column_names: Arc<Vec<String>>,
    col_count: usize,
) -> Result<CustomDbRow, SqlMiddlewareDbError> {
    let mut row_values = Vec::with_capacity(col_count);
    for idx in 0..col_count {
        row_values.push(sqlite_extract_value_sync(row, idx)?);
    }
    Ok(CustomDbRow::new(column_names, row_values))
}
