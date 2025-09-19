use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Build a result set from a Turso query execution
pub async fn build_result_set(
    mut rows: turso::Rows,
    column_names: Option<std::sync::Arc<Vec<String>>>,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut result_set = ResultSet::with_capacity(16);

    if let Some(cols) = column_names.clone() {
        result_set.set_column_names(cols);
    }

    // Iterate through rows
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("Turso row fetch error: {e}")))?
    {
        // Fetch columns lazily: if not provided, infer positions only
        let mut values: Vec<RowValues> = Vec::with_capacity(row.column_count());

        for idx in 0..row.column_count() {
            let value = row.get_value(idx).map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Turso value conversion error: {e}"))
            })?;

            let rv = match value {
                turso::Value::Null => RowValues::Null,
                turso::Value::Integer(i) => RowValues::Int(i),
                turso::Value::Real(f) => RowValues::Float(f),
                turso::Value::Text(s) => RowValues::Text(s),
                turso::Value::Blob(b) => RowValues::Blob(b),
            };
            values.push(rv);
        }

        result_set.add_row_values(values);
    }

    Ok(result_set)
}
