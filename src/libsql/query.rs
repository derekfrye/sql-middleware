use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Build a result set from a libsql query execution
pub async fn build_result_set(
    mut rows: deadpool_libsql::libsql::Rows,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Get column count to build column names
    let column_count = rows.column_count();
    let mut column_names =
        Vec::with_capacity(usize::try_from(column_count).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Invalid column count: {e}"))
        })?);

    for i in 0..column_count {
        if let Some(name) = rows.column_name(i) {
            column_names.push(name.to_string());
        } else {
            column_names.push(format!("column_{i}"));
        }
    }

    // Create result set with column names
    let mut result_set = ResultSet::with_capacity(10);
    let column_names_rc = std::sync::Arc::new(column_names);
    result_set.set_column_names(column_names_rc);

    // Process each row
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Failed to get next row: {e}")))?
    {
        let mut row_values = Vec::new();

        let col_count = result_set
            .get_column_names()
            .ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError("No column names available".to_string())
            })?
            .len();

        for i in 0..col_count {
            let value = libsql_extract_value(
                &row,
                i32::try_from(i).map_err(|e| {
                    SqlMiddlewareDbError::ExecutionError(format!("Invalid column index: {e}"))
                })?,
            )?;
            row_values.push(value);
        }

        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Extract a `RowValues` from a libsql row at the given index
fn libsql_extract_value(
    row: &deadpool_libsql::libsql::Row,
    idx: i32,
) -> Result<RowValues, SqlMiddlewareDbError> {
    let value = row.get_value(idx).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to get value at index {idx}: {e}"))
    })?;

    match value {
        deadpool_libsql::libsql::Value::Null => Ok(RowValues::Null),
        deadpool_libsql::libsql::Value::Integer(i) => Ok(RowValues::Int(i)),
        deadpool_libsql::libsql::Value::Real(f) => Ok(RowValues::Float(f)),
        deadpool_libsql::libsql::Value::Text(s) => {
            // Check if it's a JSON string (simple heuristic)
            if (s.starts_with('{') && s.ends_with('}')) || (s.starts_with('[') && s.ends_with(']'))
            {
                match serde_json::from_str(&s) {
                    Ok(json_val) => Ok(RowValues::JSON(json_val)),
                    Err(_) => Ok(RowValues::Text(s)), // If JSON parsing fails, treat as text
                }
            } else {
                // Try to parse as timestamp
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%F %T%.f") {
                    Ok(RowValues::Timestamp(dt))
                } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%F %T") {
                    Ok(RowValues::Timestamp(dt))
                } else {
                    Ok(RowValues::Text(s))
                }
            }
        }
        deadpool_libsql::libsql::Value::Blob(bytes) => Ok(RowValues::Blob(bytes)),
    }
}
