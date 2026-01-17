use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use crate::types::{ConversionMode, ParamConverter};
use chrono::NaiveDateTime;
use serde_json::Value;
use tokio_postgres::{Client, Statement, Transaction, types::ToSql};

use super::params::Params as PgParams;

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

/// Extracts a `RowValues` from a `tokio_postgres` Row at the given index.
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if the column cannot be retrieved.
pub fn postgres_extract_value(
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

/// Build a result set from raw Postgres rows (without a Transaction)
///
/// # Errors
/// Returns errors from result processing.
pub fn build_result_set_from_rows(
    rows: &[tokio_postgres::Row],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut result_set = ResultSet::with_capacity(rows.len());
    if let Some(row) = rows.first() {
        let cols: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
        result_set.set_column_names(std::sync::Arc::new(cols));
    }

    for row in rows {
        let col_count = row.columns().len();
        let mut row_values = Vec::with_capacity(col_count);
        for idx in 0..col_count {
            row_values.push(postgres_extract_value(row, idx)?);
        }
        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Build a result set using statement metadata for column names.
///
/// # Errors
/// Returns errors from row value extraction.
pub fn build_result_set_from_statement(
    stmt: &Statement,
    rows: &[tokio_postgres::Row],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|col| col.name().to_string())
        .collect();
    let column_count = column_names.len();

    let mut result_set = ResultSet::with_capacity(rows.len());
    result_set.set_column_names(std::sync::Arc::new(column_names));

    for row in rows {
        let mut row_values = Vec::with_capacity(column_count);
        for idx in 0..column_count {
            row_values.push(postgres_extract_value(row, idx)?);
        }
        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Execute a SELECT query on a client without managing transactions
///
/// # Errors
/// Returns errors from parameter conversion or query execution.
pub async fn execute_query_on_client(
    client: &Client,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = PgParams::convert_sql_params(params, ConversionMode::Query)?;
    let rows = client
        .query(query, converted.as_refs())
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres select error: {e}")))?;
    build_result_set_from_rows(&rows)
}

/// Execute a prepared SELECT query on a client without managing transactions.
///
/// # Errors
/// Returns errors from parameter conversion, preparation, or query execution.
pub async fn execute_query_prepared_on_client(
    client: &Client,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let stmt = client.prepare(query).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("postgres prepare error: {e}"))
    })?;
    let converted = PgParams::convert_sql_params(params, ConversionMode::Query)?;
    let rows = client
        .query(&stmt, converted.as_refs())
        .await
        .map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres select error: {e}"))
        })?;
    build_result_set_from_statement(&stmt, &rows)
}

/// Execute a DML query on a client without managing transactions
///
/// # Errors
/// Returns errors from parameter conversion or query execution.
pub async fn execute_dml_on_client(
    client: &Client,
    query: &str,
    params: &[RowValues],
    err_label: &str,
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = PgParams::convert_sql_params(params, ConversionMode::Execute)?;
    let rows = client
        .execute(query, converted.as_refs())
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("{err_label}: {e}")))?;
    usize::try_from(rows).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!(
            "postgres affected rows conversion error: {e}"
        ))
    })
}

/// Execute a prepared DML query on a client without managing transactions.
///
/// # Errors
/// Returns errors from parameter conversion, preparation, or query execution.
pub async fn execute_dml_prepared_on_client(
    client: &Client,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let stmt = client.prepare(query).await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("postgres prepare error: {e}"))
    })?;
    let converted = PgParams::convert_sql_params(params, ConversionMode::Execute)?;
    let rows = client
        .execute(&stmt, converted.as_refs())
        .await
        .map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("postgres execute error: {e}"))
        })?;
    usize::try_from(rows).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!(
            "postgres affected rows conversion error: {e}"
        ))
    })
}
