use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use std::sync::Arc;

/// Convert a Turso Value to our middleware RowValues
pub fn turso_value_to_row_value(value: &turso::Value) -> RowValues {
    match value {
        turso::Value::Null => RowValues::Null,
        turso::Value::Integer(i) => RowValues::Int(*i),
        turso::Value::Real(f) => RowValues::Float(*f),
        turso::Value::Text(s) => RowValues::Text(s.clone()),
        turso::Value::Blob(bytes) => RowValues::Blob(bytes.clone()),
    }
}

/// Build a ResultSet from Turso Rows
pub async fn build_result_set_from_rows(
    mut rows: turso::Rows,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut result_set = ResultSet::with_capacity(0);
    let mut column_names_arc: Option<Arc<Vec<String>>> = None;

    while let Some(row) = rows.next().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("Failed to read row: {e}"))
    })? {
        // Extract column names from the first row if not done yet
        if column_names_arc.is_none() {
            // Use a simple approach - try to access a few values to determine column count
            let mut column_count = 0;
            while let Ok(_) = row.get_value(column_count) {
                column_count += 1;
                if column_count > 100 { // Safety break to avoid infinite loop
                    break;
                }
            }
            
            let column_names: Vec<String> = (0..column_count)
                .map(|i| format!("column_{}", i))
                .collect();
            column_names_arc = Some(Arc::new(column_names));
            result_set.set_column_names(column_names_arc.as_ref().unwrap().clone());
        }

        let mut values = Vec::new();
        let column_count = column_names_arc.as_ref().unwrap().len();
        
        // Extract values from the row
        for i in 0..column_count {
            let value = row.get_value(i).map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("Failed to get value at index {i}: {e}"))
            })?;
            values.push(turso_value_to_row_value(&value));
        }

        result_set.add_row_values(values);
    }

    Ok(result_set)
}