use super::transaction::{Tx, begin_transaction};
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use std::ops::DerefMut;
use tokio_postgres::Client;

/// Execute a batch of SQL statements for Postgres
///
/// # Errors
/// Returns errors from transaction operations or batch execution.
pub async fn execute_batch<C>(pg_client: &mut C, query: &str) -> Result<(), SqlMiddlewareDbError>
where
    C: DerefMut<Target = Client>,
{
    let tx: Tx<'_> = begin_transaction(pg_client).await?;
    tx.execute_batch(query).await?;
    tx.commit().await?;

    Ok(())
}

/// Execute a SELECT query with parameters
///
/// # Errors
/// Returns errors from parameter conversion, transaction operations, query preparation, or result set building.
pub async fn execute_select<C>(
    pg_client: &mut C,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError>
where
    C: DerefMut<Target = Client>,
{
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
pub async fn execute_dml<C>(
    pg_client: &mut C,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError>
where
    C: DerefMut<Target = Client>,
{
    let tx: Tx<'_> = begin_transaction(pg_client).await?;
    let prepared = tx.prepare(query).await?;
    let rows = tx.execute_prepared(&prepared, params).await?;
    tx.commit().await?;
    Ok(rows)
}
