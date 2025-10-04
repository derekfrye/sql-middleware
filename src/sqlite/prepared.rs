use std::sync::Arc;

use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

use super::params::convert_params;
use super::worker::SqliteConnection;

/// Handle to a prepared SQLite statement owned by a worker connection.
///
/// Instances can be cloned and reused across awaited calls. Dropping the handle
/// simply releases the reference; the underlying connection will keep the
/// statement cached via `rusqlite`'s internal `prepare_cached` mechanism.
#[derive(Clone, Debug)]
pub struct SqlitePreparedStatement {
    connection: SqliteConnection,
    query: Arc<String>,
}

impl SqlitePreparedStatement {
    pub(crate) fn new(connection: SqliteConnection, query: Arc<String>) -> Self {
        Self { connection, query }
    }

    /// Execute the prepared statement as a query and materialise the rows into a [`ResultSet`].
    pub async fn query(&self, params: &[RowValues]) -> Result<ResultSet, SqlMiddlewareDbError> {
        let params_owned = convert_params(params);
        self.connection
            .execute_prepared_select(Arc::clone(&self.query), params_owned)
            .await
    }

    /// Execute the prepared statement as a DML (INSERT/UPDATE/DELETE) returning rows affected.
    pub async fn execute(&self, params: &[RowValues]) -> Result<usize, SqlMiddlewareDbError> {
        let params_owned = convert_params(params);
        self.connection
            .execute_prepared_dml(Arc::clone(&self.query), params_owned)
            .await
    }

    /// Access the raw SQL string of the prepared statement.
    pub fn sql(&self) -> &str {
        self.query.as_str()
    }
}
