use chrono::NaiveDateTime;
use futures_util::TryStreamExt;
use tiberius::Query;

use super::config::MssqlClient;
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

/// Build a result set from a SQL Server query execution
pub async fn build_result_set(
    client: &mut MssqlClient,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Use the shared function to prepare and bind the query
    let query_builder = bind_query_params(query, params);

    // Execute the query
    let mut stream = query_builder.query(client).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("SQL Server query error: {e}"))
    })?;

    // Get column information
    let columns_opt = stream.columns().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("SQL Server column fetch error: {e}"))
    })?;

    let columns = columns_opt.ok_or_else(|| {
        SqlMiddlewareDbError::ExecutionError("No columns returned from query".to_string())
    })?;

    let column_names: Vec<String> = columns.iter().map(|col| col.name().to_string()).collect();

    // Preallocate capacity if we can estimate the number of rows
    let mut result_set = ResultSet::with_capacity(10);
    // Store column names once in the result set
    let column_names_rc = std::sync::Arc::new(column_names);
    result_set.set_column_names(column_names_rc);

    // Process the stream
    let mut rows_stream = stream.into_row_stream();
    while let Some(row_result) = rows_stream.try_next().await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("SQL Server row fetch error: {e}"))
    })? {
        let col_count = result_set
            .get_column_names()
            .ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError("No column names available".to_string())
            })?
            .len();

        let mut row_values = Vec::with_capacity(col_count);

        for i in 0..col_count {
            // Extract values from the row
            if let Some(value) = extract_value(&row_result, i)? {
                row_values.push(value);
            } else {
                row_values.push(RowValues::Null);
            }
        }

        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Extract a value from a row at a specific index
fn extract_value(
    row: &tiberius::Row,
    idx: usize,
) -> Result<Option<RowValues>, SqlMiddlewareDbError> {
    // Since Tiberius Row API is a bit complex and varies by version,
    // we'll use a simple approach by trying different value types

    // Try integer
    if let Ok(Some(val)) = row.try_get::<i32, _>(idx) {
        return Ok(Some(RowValues::Int(i64::from(val))));
    }

    if let Ok(Some(val)) = row.try_get::<i64, _>(idx) {
        return Ok(Some(RowValues::Int(val)));
    }

    // Try floating point
    if let Ok(Some(val)) = row.try_get::<f32, _>(idx) {
        return Ok(Some(RowValues::Float(f64::from(val))));
    }

    if let Ok(Some(val)) = row.try_get::<f64, _>(idx) {
        return Ok(Some(RowValues::Float(val)));
    }

    // Try boolean
    if let Ok(Some(val)) = row.try_get::<bool, _>(idx) {
        return Ok(Some(RowValues::Bool(val)));
    }

    // Try string (most values can be represented as strings)
    if let Ok(Some(val)) = row.try_get::<&str, _>(idx) {
        // If it looks like a date/time, try to parse it
        if val.contains('-') && (val.contains(':') || val.contains(' ')) {
            if let Ok(dt) = NaiveDateTime::parse_from_str(val, "%Y-%m-%d %H:%M:%S%.f") {
                return Ok(Some(RowValues::Timestamp(dt)));
            } else if let Ok(dt) = NaiveDateTime::parse_from_str(val, "%Y-%m-%d %H:%M:%S") {
                return Ok(Some(RowValues::Timestamp(dt)));
            }
        }

        // Otherwise, just return as text
        return Ok(Some(RowValues::Text(val.to_string())));
    }

    // Try bytes (binary data)
    if let Ok(Some(val)) = row.try_get::<&[u8], _>(idx) {
        return Ok(Some(RowValues::Blob(val.to_vec())));
    }

    // Check if the value is NULL
    if let Ok(None) = row.try_get::<&str, _>(idx) {
        return Ok(None);
    }

    // If none of the above worked, return NULL
    Ok(None)
}

/// Bind parameters directly to the query for SQL Server
/// Return a query builder with parameters already bound
pub fn bind_query_params<'a>(query: &'a str, params: &[RowValues]) -> Query<'a> {
    // Create the query builder
    let mut query_builder = Query::new(query);

    // Bind parameters directly - not using OwnedParam as intermediary
    // since tiberius Query will own the data
    for param in params {
        match param {
            RowValues::Int(i) => query_builder.bind(*i),
            RowValues::Float(f) => query_builder.bind(*f),
            RowValues::Text(s) => query_builder.bind(s.clone()),
            RowValues::Bool(b) => query_builder.bind(*b),
            RowValues::Timestamp(dt) => {
                // Format timestamps efficiently
                let formatted = dt.format("%Y-%m-%dT%H:%M:%S%.f").to_string();
                query_builder.bind(formatted);
            }
            RowValues::Null => query_builder.bind(Option::<String>::None),
            RowValues::JSON(jsval) => query_builder.bind(jsval.to_string()),
            RowValues::Blob(bytes) => query_builder.bind(bytes.clone()),
        }
    }

    query_builder
}
