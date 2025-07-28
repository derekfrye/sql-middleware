use super::params::Params;
use super::query::build_result_set;
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use deadpool_postgres::Object;

/// Execute a batch of SQL statements for Postgres
///
/// # Errors
/// Returns errors from transaction operations or batch execution.
pub async fn execute_batch(
    pg_client: &mut Object,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    // Begin a transaction
    let tx = pg_client.transaction().await?;

    // Execute the batch of queries
    tx.batch_execute(query).await?;

    // Commit the transaction
    tx.commit().await?;

    Ok(())
}

/// Execute a SELECT query with parameters
///
/// # Errors
/// Returns errors from parameter conversion, transaction operations, query preparation, or result set building.
pub async fn execute_select(
    pg_client: &mut Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let params = Params::convert(params)?;
    let tx = pg_client.transaction().await?;
    let stmt = tx.prepare(query).await?;
    let result_set = build_result_set(&stmt, params.as_refs(), &tx).await?;
    tx.commit().await?;
    Ok(result_set)
}

/// Execute a DML query (INSERT, UPDATE, DELETE) with parameters
///
/// # Errors
/// Returns errors from parameter conversion, transaction operations, or query execution.
pub async fn execute_dml(
    pg_client: &mut Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let params = Params::convert(params)?;
    let tx = pg_client.transaction().await?;

    let stmt = tx.prepare(query).await?;
    let rows = tx.execute(&stmt, params.as_refs()).await?;
    tx.commit().await?;

    usize::try_from(rows).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Invalid rows affected count: {e}"))
    })
}
