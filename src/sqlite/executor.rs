use super::params::convert_params;
use super::worker::SqliteConnection;
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Execute a batch of SQL statements for `SQLite`
pub async fn execute_batch(
    sqlite_client: &SqliteConnection,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    sqlite_client.execute_batch(query.to_owned()).await
}

/// Execute a SELECT query in `SQLite`
pub async fn execute_select(
    sqlite_client: &SqliteConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let params_owned = convert_params(params);
    sqlite_client
        .execute_select(query.to_owned(), params_owned)
        .await
}

/// Execute a DML query (INSERT, UPDATE, DELETE) in `SQLite`
pub async fn execute_dml(
    sqlite_client: &SqliteConnection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let params_owned = convert_params(params);
    sqlite_client
        .execute_dml(query.to_owned(), params_owned)
        .await
}
