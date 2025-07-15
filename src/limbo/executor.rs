use crate::limbo::query::build_result_set_from_rows;
use crate::middleware::{ResultSet, SqlMiddlewareDbError};

/// Execute a SELECT query with parameters
pub async fn execute_select(
    conn: &turso::Connection,
    query: &str,
    params: Vec<turso::Value>,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let rows = conn
        .query(query, params)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso query failed: {e}")))?;

    build_result_set_from_rows(rows).await
}

/// Execute a DML statement (INSERT, UPDATE, DELETE) with parameters
pub async fn execute_dml(
    conn: &turso::Connection,
    query: &str,
    params: Vec<turso::Value>,
) -> Result<u64, SqlMiddlewareDbError> {
    let rows_affected = conn
        .execute(query, params)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso execute failed: {e}")))?;

    Ok(rows_affected)
}

/// Execute a batch of statements
pub async fn execute_batch(
    conn: &turso::Connection,
    statements: &[&str],
) -> Result<(), SqlMiddlewareDbError> {
    for statement in statements {
        conn.execute(statement, ())
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso batch execute failed: {e}")))?;
    }
    Ok(())
}

/// Execute a batch of statements as a single string
pub async fn execute_batch_string(
    conn: &turso::Connection,
    batch_sql: &str,
) -> Result<(), SqlMiddlewareDbError> {
    // Split the batch SQL into individual statements
    let statements: Vec<&str> = batch_sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    execute_batch(conn, &statements).await
}