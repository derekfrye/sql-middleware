use crate::adapters::params::convert_params;
use crate::middleware::{ConversionMode, ResultSet, RowValues, SqlMiddlewareDbError};

use super::connection::SqliteConnection;
use super::params::Params;
use super::query::build_result_set;

/// Execute a batch of SQL statements for `SQLite` using auto-commit.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails.
pub async fn execute_batch(
    sqlite_client: &mut SqliteConnection,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    sqlite_client.execute_batch(query).await
}

/// Execute a SELECT query in `SQLite`.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution or result processing fails.
pub async fn execute_select(
    sqlite_client: &mut SqliteConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let params_owned = convert_params::<Params>(params, ConversionMode::Query)?.0;
    sqlite_client
        .execute_select(query, &params_owned, build_result_set)
        .await
}

/// Execute a DML query (INSERT, UPDATE, DELETE) in `SQLite`.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails or rows affected cannot be converted.
pub async fn execute_dml(
    sqlite_client: &mut SqliteConnection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let params_owned = convert_params::<Params>(params, ConversionMode::Execute)?.0;
    sqlite_client.execute_dml(query, &params_owned).await
}
