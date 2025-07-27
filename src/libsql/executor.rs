use super::params::Params;
use super::query::build_result_set;
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use deadpool_libsql::Object;

/// Execute a batch of SQL statements for libsql
pub async fn execute_batch(
    libsql_client: &Object,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    // Begin a transaction
    let tx = libsql_client.transaction().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to begin transaction: {e}"))
    })?;

    // Execute the batch of queries
    tx.execute_batch(query).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to execute batch: {e}"))
    })?;

    // Commit the transaction
    tx.commit().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to commit transaction: {e}"))
    })?;

    Ok(())
}

/// Execute a SELECT query with parameters
pub async fn execute_select(
    libsql_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let params = Params::convert(params)?;

    // Execute the query
    let rows = libsql_client
        .query(query, params.into_vec())
        .await
        .map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Failed to execute query: {e}"))
        })?;

    // Build result set from rows
    build_result_set(rows).await
}

/// Execute a DML query (INSERT, UPDATE, DELETE) with parameters
pub async fn execute_dml(
    libsql_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let params = Params::convert(params)?;

    // Begin a transaction for DML operations
    let tx = libsql_client.transaction().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to begin transaction: {e}"))
    })?;

    // Execute the DML query
    let rows_affected = tx
        .execute(query, params.into_vec())
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Failed to execute DML: {e}")))?;

    // Commit the transaction
    tx.commit().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to commit transaction: {e}"))
    })?;

    Ok(rows_affected as usize)
}
