use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite::ToSql;

use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use super::params::convert_params;
use super::query::build_result_set;

/// Execute a batch of SQL statements for SQLite
pub async fn execute_batch(
    sqlite_client: &Object,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let query_owned = query.to_owned();

    // Use interact to run the blocking code in a separate thread.
    sqlite_client
        .interact(move |conn| {
            // Begin a transaction
            let tx = conn.transaction()?;

            // Execute the batch of queries
            tx.execute_batch(&query_owned)?;

            // Commit the transaction
            tx.commit()?;

            Ok(())
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Interact error: {}", e)))?
}

/// Execute a SELECT query in SQLite
pub async fn execute_select(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread and return the result directly
    sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let mut stmt = conn.prepare(&query_owned)?;

            // Execute the query
            build_result_set(&mut stmt, &params_owned)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Interact error: {}", e)))?
}

/// Execute a DML query (INSERT, UPDATE, DELETE) in SQLite
pub async fn execute_dml(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread.
    sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let tx = conn.transaction()?;

            let param_refs: Vec<&dyn ToSql> =
                params_owned.iter().map(|v| v as &dyn ToSql).collect();

            // Execute the query
            let rows = {
                let mut stmt = tx.prepare(&query_owned)?;
                stmt.execute(&param_refs[..])?
            };
            tx.commit()?;

            Ok(rows)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Interact error: {}", e)))?
}