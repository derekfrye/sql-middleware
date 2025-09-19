use crate::middleware::{ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError};
use crate::turso::params::Params as TursoParams;

/// Execute a batch of SQL statements for Turso
pub async fn execute_batch(
    turso_conn: &turso::Connection,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    turso_conn
        .execute_batch(query)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso execute_batch error: {e}")))
}

/// Execute a SELECT query for Turso and return a `ResultSet`
pub async fn execute_select(
    turso_conn: &turso::Connection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Convert params
    let converted = <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;

    // Prepare to fetch column names
    let mut stmt = turso_conn
        .prepare(query)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso prepare error: {e}")))?;

    let cols = stmt
        .columns()
        .into_iter()
        .map(|c| c.name().to_string())
        .collect::<Vec<_>>();
    let cols_arc = std::sync::Arc::new(cols);

    // Run query using the same statement to avoid double-prepare
    let rows = stmt
        .query(converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso query error: {e}")))?;

    crate::turso::query::build_result_set(rows, Some(cols_arc)).await
}

/// Execute a DML statement for Turso and return affected row count
pub async fn execute_dml(
    turso_conn: &turso::Connection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = <TursoParams as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;
    let affected = turso_conn
        .execute(query, converted.0)
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso execute error: {e}")))?;
    usize::try_from(affected).map_err(|e| SqlMiddlewareDbError::ExecutionError(format!(
        "Turso affected rows conversion error: {e}"
    )))
}

