use std::sync::Arc;

use tokio::sync::Mutex;

use crate::middleware::{CustomDbRow, ResultSet, RowValues, SqlMiddlewareDbError};

use super::{
    config::MssqlClient,
    query::{build_result_set, query_map_optional},
};

/// Prepared statement wrapper for SQL Server that holds onto a single connection.
///
/// This mirrors the non-transaction prepared handle exposed by other backends so
/// the public API stays consistent. **Tiberius does not expose a real prepared
/// statement type or caching**, so this wrapper simply stores the SQL text and
/// re-binds parameters on each execution against the same connection. It does
/// *not* amortize server-side compilation the way true prepared statements
/// would. Use this for API parity and connection pinning, not for preparation
/// cache wins.
#[derive(Clone)]
pub struct MssqlNonTxPreparedStatement {
    client: Arc<Mutex<MssqlClient>>,
    sql: Arc<String>,
}

impl std::fmt::Debug for MssqlNonTxPreparedStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MssqlNonTxPreparedStatement")
            .field("client", &"<MssqlClient>")
            .field("sql", &self.sql)
            .finish()
    }
}

impl MssqlNonTxPreparedStatement {
    /// Prepare a statement on the provided MSSQL client.
    ///
    /// The returned handle owns the client, so it should be created from a
    /// dedicated connection (e.g., via `create_mssql_client`).
    pub fn prepare(client: MssqlClient, sql: &str) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
            sql: Arc::new(sql.to_owned()),
        }
    }

    /// Start configuring a prepared SELECT execution.
    #[must_use]
    pub fn select(&self) -> MssqlPreparedSelect<'_, '_> {
        MssqlPreparedSelect {
            statement: self,
            params: &[],
        }
    }

    /// Start configuring a prepared DML execution.
    #[must_use]
    pub fn execute(&self) -> MssqlPreparedExecute<'_, '_> {
        MssqlPreparedExecute {
            statement: self,
            params: &[],
        }
    }

    /// Execute the prepared statement as a query and materialize results.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result
    /// construction fails.
    pub(crate) async fn query(
        &self,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let mut client = self.client.lock().await;
        build_result_set(&mut client, &self.sql, params).await
    }

    /// Execute the prepared statement as a query and return the first row, if present.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result construction fails.
    pub(crate) async fn query_optional(
        &self,
        params: &[RowValues],
    ) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.query(params).await.map(ResultSet::into_optional)
    }

    /// Execute the prepared statement as a query and return the first row.
    ///
    /// # Errors
    /// Returns an error if execution fails or no row is returned.
    pub(crate) async fn query_one(
        &self,
        params: &[RowValues],
    ) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.query(params).await?.into_one()
    }

    /// Execute the prepared statement and map the first native SQL Server row.
    ///
    /// Use this for hot paths that only need one row and can decode directly from
    /// `tiberius::Row`, avoiding `ResultSet` materialisation.
    ///
    /// # Errors
    /// Returns an error if execution fails, no row is returned, or the mapper fails.
    pub(crate) async fn query_map_one<T, F>(
        &self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.query_map_optional(params, mapper)
            .await?
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("query returned no rows".into()))
    }

    /// Execute the prepared statement and map the first native SQL Server row, returning `None` if
    /// no row exists.
    ///
    /// # Errors
    /// Returns an error if execution or the mapper fails.
    pub(crate) async fn query_map_optional<T, F>(
        &self,
        params: &[RowValues],
        mapper: F,
    ) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        let mut client = self.client.lock().await;
        query_map_optional(&mut client, &self.sql, params, mapper).await
    }

    /// Execute the prepared statement as DML and return affected rows.
    ///
    /// # Errors
    /// Returns an error if parameter conversion or execution fails.
    pub(crate) async fn execute_values(
        &self,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let mut client = self.client.lock().await;
        let query_builder = super::query::bind_query_params(&self.sql, params);
        let exec_result = query_builder.execute(&mut *client).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL prepared execute error: {e}"))
        })?;

        let rows_affected: u64 = exec_result.rows_affected().iter().sum();
        usize::try_from(rows_affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Invalid rows affected count: {e}"))
        })
    }

    /// Access the SQL text.
    #[must_use]
    pub fn sql(&self) -> &str {
        self.sql.as_str()
    }
}

/// Builder for executing a prepared SQL Server DML statement.
pub struct MssqlPreparedExecute<'stmt, 'params> {
    statement: &'stmt MssqlNonTxPreparedStatement,
    params: &'params [RowValues],
}

impl<'stmt, 'params> MssqlPreparedExecute<'stmt, 'params> {
    /// Use middleware `RowValues` parameters.
    #[must_use]
    pub fn params<'next>(self, params: &'next [RowValues]) -> MssqlPreparedExecute<'stmt, 'next> {
        MssqlPreparedExecute {
            statement: self.statement,
            params,
        }
    }

    /// Execute the DML statement and return affected rows.
    ///
    /// # Errors
    /// Returns an error if parameter conversion or execution fails.
    pub async fn run(self) -> Result<usize, SqlMiddlewareDbError> {
        self.statement.execute_values(self.params).await
    }
}

/// Builder for executing a prepared SQL Server SELECT.
pub struct MssqlPreparedSelect<'stmt, 'params> {
    statement: &'stmt MssqlNonTxPreparedStatement,
    params: &'params [RowValues],
}

impl<'stmt, 'params> MssqlPreparedSelect<'stmt, 'params> {
    /// Use middleware `RowValues` parameters.
    #[must_use]
    pub fn params<'next>(self, params: &'next [RowValues]) -> MssqlPreparedSelect<'stmt, 'next> {
        MssqlPreparedSelect {
            statement: self.statement,
            params,
        }
    }

    /// Execute and return all rows as a `ResultSet`.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result construction fails.
    pub async fn all(self) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.statement.query(self.params).await
    }

    /// Execute and return the first row, if present.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result construction fails.
    pub async fn optional(self) -> Result<Option<CustomDbRow>, SqlMiddlewareDbError> {
        self.statement.query_optional(self.params).await
    }

    /// Execute and return exactly one row.
    ///
    /// # Errors
    /// Returns an error if execution fails or no row is returned.
    pub async fn one(self) -> Result<CustomDbRow, SqlMiddlewareDbError> {
        self.statement.query_one(self.params).await
    }

    /// Execute and map exactly one native SQL Server row.
    ///
    /// # Errors
    /// Returns an error if execution fails, no row is returned, or the mapper fails.
    pub async fn map_one<T, F>(self, mapper: F) -> Result<T, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.statement.query_map_one(self.params, mapper).await
    }

    /// Execute and map the first native SQL Server row, if present.
    ///
    /// # Errors
    /// Returns an error if execution or the mapper fails.
    pub async fn map_optional<T, F>(self, mapper: F) -> Result<Option<T>, SqlMiddlewareDbError>
    where
        F: FnOnce(&tiberius::Row) -> Result<T, SqlMiddlewareDbError>,
    {
        self.statement.query_map_optional(self.params, mapper).await
    }
}
