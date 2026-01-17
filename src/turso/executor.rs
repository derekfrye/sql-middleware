use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};
use crate::query_utils::extract_column_names;
use crate::turso::params::Params as TursoParams;

/// Execute a batch of SQL statements for Turso
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError` when the underlying Turso batch execution fails.
pub async fn execute_batch(
    turso_conn: &turso::Connection,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    turso_conn.execute_batch(query).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Turso execute_batch error: {e}"))
    })
}

/// Execute a SELECT query for Turso and return a `ResultSet`
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError` when preparing or running the query fails, or when
/// converting rows into the middleware `ResultSet` fails.
pub async fn execute_select(
    turso_conn: &turso::Connection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Convert params
    let converted =
        <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;

    // Prepare to fetch column names
    let mut stmt = turso_conn
        .prepare(query)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso prepare error: {e}")))?;

    let cols = extract_column_names(stmt.columns().iter(), |col| col.name());
    let cols_arc = std::sync::Arc::new(cols);

    // Run query using the same statement to avoid double-prepare
    let rows = stmt
        .query(converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso query error: {e}")))?;

    crate::turso::query::build_result_set(rows, Some(cols_arc)).await
}

/// Execute a DML statement for Turso and return affected row count
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError` when executing the statement fails or the affected row
/// count cannot be converted to `usize`.
pub async fn execute_dml(
    turso_conn: &turso::Connection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted =
        <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
    let affected = turso_conn
        .execute(query, converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso execute error: {e}")))?;
    usize::try_from(affected).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Turso affected rows conversion error: {e}"))
    })
}
