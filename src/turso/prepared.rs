use std::sync::Arc;

use tokio::sync::Mutex;

use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};

use super::params::Params as TursoParams;

/// Handle to a prepared Turso statement owned by a pooled connection.
///
/// Instances can be cloned and reused across awaited calls. Internally, the
/// underlying `turso::Statement` is protected by a `tokio::sync::Mutex` so the
/// compiled statement can be shared safely between tasks while still benefiting
/// from Turso's statement caching.
#[derive(Clone)]
pub struct TursoPreparedStatement {
    _connection: turso::Connection,
    statement: Arc<Mutex<turso::Statement>>,
    columns: Arc<Vec<String>>,
    sql: Arc<String>,
}

impl std::fmt::Debug for TursoPreparedStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TursoPreparedStatement")
            .field("_connection", &"<turso::Connection>")
            .field("statement", &"<turso::Statement>")
            .field("columns", &self.columns)
            .field("sql", &self.sql)
            .finish()
    }
}

impl TursoPreparedStatement {
    pub(crate) async fn prepare(
        connection: turso::Connection,
        sql: &str,
    ) -> Result<Self, SqlMiddlewareDbError> {
        let sql_arc = Arc::new(sql.to_owned());
        let statement = connection.prepare(sql).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Turso prepare error: {e}"))
        })?;

        let columns = statement
            .columns()
            .into_iter()
            .map(|c| c.name().to_string())
            .collect::<Vec<_>>();

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
        let converted =
            <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;

        let rows = {
            let mut stmt = self.statement.lock().await;
            stmt.query(converted.0).await.map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso prepared query error: {e}"))
            })?
        };

        let result = crate::turso::query::build_result_set(rows, Some(self.columns.clone())).await;

        self.reset().await;
        result
    }

    /// Execute the prepared statement as a DML (INSERT/UPDATE/DELETE) returning rows affected.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if parameter conversion fails, Turso returns an execution
    /// error, or the affected-row count cannot be converted into `usize`.
    pub async fn execute(&self, params: &[RowValues]) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;

        let affected = {
            let mut stmt = self.statement.lock().await;
            let affected = stmt.execute(converted.0).await.map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso prepared execute error: {e}"))
            })?;
            stmt.reset();
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

    async fn reset(&self) {
        let stmt = self.statement.lock().await;
        stmt.reset();
    }
}
