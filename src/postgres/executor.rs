use super::transaction::{begin_transaction, Tx};
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
    let tx: Tx<'_> = begin_transaction(pg_client).await?;
    tx.execute_batch(query).await?;
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
    let tx: Tx<'_> = begin_transaction(pg_client).await?;
    let prepared = tx.prepare(query).await?;
    let result_set = tx.query_prepared(&prepared, params).await?;
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
    let tx: Tx<'_> = begin_transaction(pg_client).await?;
    let prepared = tx.prepare(query).await?;
    let rows = tx.execute_prepared(&prepared, params).await?;
    tx.commit().await?;
    Ok(rows)
}
