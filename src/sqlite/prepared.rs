use std::sync::Arc;

use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

use super::connection::SqliteConnection;
use super::params::Params;

/// Handle to a prepared `SQLite` statement tied to a connection.
pub struct SqlitePreparedStatement<'conn> {
    connection: &'conn mut SqliteConnection,
    query: Arc<String>,
}

impl<'conn> SqlitePreparedStatement<'conn> {
    pub(crate) fn new(connection: &'conn mut SqliteConnection, query: Arc<String>) -> Self {
        Self { connection, query }
    }

    /// Execute the prepared statement as a query and materialise the rows into a [`ResultSet`].
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or result conversion encounters an issue.
    pub async fn query(&mut self, params: &[RowValues]) -> Result<ResultSet, SqlMiddlewareDbError> {
        let params_owned = Params::convert(params)?.0;
        self.connection
            .execute_select(self.query.as_ref(), &params_owned, super::query::build_result_set)
            .await
    }

    /// Execute the prepared statement as a DML (INSERT/UPDATE/DELETE) returning rows affected.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError`] if execution fails or if the result cannot be converted into the expected row count.
    pub async fn execute(&mut self, params: &[RowValues]) -> Result<usize, SqlMiddlewareDbError> {
        let params_owned = Params::convert(params)?.0;
        self.connection
            .execute_dml(self.query.as_ref(), &params_owned)
            .await
    }

    /// Access the raw SQL string of the prepared statement.
    #[must_use]
    pub fn sql(&self) -> &str {
        self.query.as_str()
    }
}

