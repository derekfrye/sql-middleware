use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use chrono::NaiveDateTime;
use deadpool_postgres::Transaction;
use serde_json::Value;
use tokio_postgres::{Statement, types::ToSql};

/// Build a result set from a Postgres query execution
///
/// # Errors
/// Returns errors from query execution or result processing.
pub async fn build_result_set(
    stmt: &Statement,
    params: &[&(dyn ToSql + Sync)],
    transaction: &Transaction<'_>,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Execute the query
    let rows = transaction.query(stmt, params).await?;

    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|col| col.name().to_string())
        .collect();

    // Preallocate capacity if we can estimate the number of rows
    let capacity = rows.len();
    let mut result_set = ResultSet::with_capacity(capacity);
    // Store column names once in the result set
    let column_names_rc = std::sync::Arc::new(column_names);
    result_set.set_column_names(column_names_rc);

    for row in rows {
        let mut row_values = Vec::new();

        let col_count = result_set
            .get_column_names()
            .ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError("No column names available".to_string())
            })?
            .len();

        for i in 0..col_count {
            let value = postgres_extract_value(&row, i)?;
            row_values.push(value);
        }

        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Extracts a `RowValues` from a `tokio_postgres` Row at the given index
fn postgres_extract_value(
    row: &tokio_postgres::Row,
    idx: usize,
) -> Result<RowValues, SqlMiddlewareDbError> {
    // Determine the type of the column and extract accordingly
    let type_info = row.columns()[idx].type_();

    // Match on the type based on PostgreSQL type OIDs or names
    // For simplicity, we'll handle common types. You may need to expand this.
    if type_info.name() == "int2" {
        let val: Option<i16> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, |v| RowValues::Int(i64::from(v))))
    } else if type_info.name() == "int4" {
        let val: Option<i32> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, |v| RowValues::Int(i64::from(v))))
    } else if type_info.name() == "int8" {
        let val: Option<i64> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Int))
    } else if type_info.name() == "float4" || type_info.name() == "float8" {
        let val: Option<f64> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Float))
    } else if type_info.name() == "bool" {
        let val: Option<bool> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Bool))
    } else if type_info.name() == "timestamp" || type_info.name() == "timestamptz" {
        let val: Option<NaiveDateTime> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Timestamp))
    } else if type_info.name() == "json" || type_info.name() == "jsonb" {
        let val: Option<Value> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::JSON))
    } else if type_info.name() == "bytea" {
        let val: Option<Vec<u8>> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Blob))
    } else if type_info.name() == "text"
        || type_info.name() == "varchar"
        || type_info.name() == "char"
    {
        let val: Option<String> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Text))
    } else {
        // For other types, attempt to get as string
        let val: Option<String> = row.try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Text))
    }
}
