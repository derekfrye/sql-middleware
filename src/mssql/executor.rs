use deadpool::managed::Object;

use super::config::MssqlManager;
use super::query::{bind_query_params, build_result_set};
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Execute a batch of SQL statements for SQL Server.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails.
#[allow(dead_code)]
pub async fn execute_batch(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    // Get a client from the object
    let client = &mut **mssql_client;

    // Execute the batch of queries
    let query_builder = tiberius::Query::new(query);
    query_builder.execute(client).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("SQL Server batch execution error: {e}"))
    })?;

    Ok(())
}

/// Execute a SELECT query with parameters.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution or result processing fails.
#[allow(dead_code)]
pub async fn execute_select(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Get a client from the object
    let client = &mut **mssql_client;

    // Use the build_result_set function to handle parameters and execution
    build_result_set(client, query, params).await
}

/// Execute a DML query (INSERT, UPDATE, DELETE) with parameters.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError::ExecutionError` if execution fails or rows affected cannot be converted.
#[allow(dead_code)]
pub async fn execute_dml(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    // Get a client from the object
    let client = &mut **mssql_client;

    // Prepare and bind the query
    let query_builder = bind_query_params(query, params);

    // Execute the query
    let exec_result = query_builder.execute(client).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("SQL Server DML execution error: {e}"))
    })?;

    // Get rows affected
    let rows_affected: u64 = exec_result.rows_affected().iter().sum();

    usize::try_from(rows_affected).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Invalid rows affected count: {e}"))
    })
}
