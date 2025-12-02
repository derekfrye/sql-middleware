use super::params::Params;
use super::worker::SqliteConnection;
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Execute a batch of SQL statements for `SQLite`.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails.
pub async fn execute_batch(
    sqlite_client: &SqliteConnection,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    sqlite_client.execute_batch(query.to_owned()).await
}

/// Execute a SELECT query in `SQLite`.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution or result processing fails.
pub async fn execute_select(
    sqlite_client: &SqliteConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let params_owned = Params::convert(params)?.0;
    sqlite_client
        .execute_select(query.to_owned(), params_owned)
        .await
}

/// Execute a DML query (INSERT, UPDATE, DELETE) in `SQLite`.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails or rows affected cannot be converted.
pub async fn execute_dml(
    sqlite_client: &SqliteConnection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let params_owned = Params::convert(params)?.0;
    sqlite_client
        .execute_dml(query.to_owned(), params_owned)
        .await
}
