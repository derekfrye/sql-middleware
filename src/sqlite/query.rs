use rusqlite::types::Value;
use rusqlite::{Statement, ToSql};

use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Extract a `RowValues` from a `SQLite` row.
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError` if the value cannot be converted.
pub fn sqlite_extract_value_sync(
    row: &rusqlite::Row,
    idx: usize,
) -> Result<RowValues, SqlMiddlewareDbError> {
    let value: Value = row.get(idx).map_err(SqlMiddlewareDbError::SqliteError)?;
    match value {
        Value::Null => Ok(RowValues::Null),
        Value::Integer(i) => Ok(RowValues::Int(i)),
        Value::Real(f) => Ok(RowValues::Float(f)),
        Value::Text(s) => Ok(RowValues::Text(s)),
        Value::Blob(b) => Ok(RowValues::Blob(b)),
    }
}

/// Build a result set from a `SQLite` query
/// Only SELECT queries return rows affected. If a DML is sent, it does run it.
/// If there's more than one query in the statement, idk which statement will be run.
///
/// # Errors
/// Returns `SqlMiddlewareDbError::ExecutionError` if query execution or result processing fails.
pub fn build_result_set(
    stmt: &mut Statement,
    params: &[Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
    let column_names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(std::string::ToString::to_string)
        .collect();

    // Store column names once in the result set
    let column_names_rc = std::sync::Arc::new(column_names);

    let mut rows_iter = stmt.query(&param_refs[..])?;
    // Create result set with default capacity
    let mut result_set = ResultSet::with_capacity(10);
    result_set.set_column_names(column_names_rc);

    while let Some(row) = rows_iter.next()? {
        let mut row_values = Vec::new();

        let col_count = result_set
            .get_column_names()
            .ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError("No column names available".to_string())
            })?
            .len();

        for i in 0..col_count {
            let value = sqlite_extract_value_sync(row, i)?;
            row_values.push(value);
        }

        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}
