use deadpool_sqlite::rusqlite;
use deadpool_sqlite::rusqlite::ParamsFromIter;
use rusqlite::types::Value;
use std::fmt::Write;

use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

// Thread-local buffer for efficient timestamp formatting
thread_local! {
    static TIMESTAMP_BUF: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(32));
}

/// Convert a single `RowValue` to a rusqlite `Value`
pub fn row_value_to_sqlite_value(value: &RowValues, for_execute: bool) -> rusqlite::types::Value {
    match value {
        RowValues::Int(i) => rusqlite::types::Value::Integer(*i),
        RowValues::Float(f) => rusqlite::types::Value::Real(*f),
        RowValues::Text(s) => {
            if for_execute {
                // For execute, we can move the owned String directly
                rusqlite::types::Value::Text(s.clone())
            } else {
                // For queries, we need to clone
                rusqlite::types::Value::Text(s.clone())
            }
        }
        RowValues::Bool(b) => rusqlite::types::Value::Integer(i64::from(*b)),
        // Format timestamps once for better performance
        RowValues::Timestamp(dt) => {
            // Use a thread_local buffer for timestamp formatting to avoid allocation
            TIMESTAMP_BUF.with(|buf| {
                let mut borrow = buf.borrow_mut();
                borrow.clear();
                // Format directly into the string buffer
                write!(borrow, "{}", dt.format("%F %T%.f")).unwrap();
                rusqlite::types::Value::Text(borrow.clone())
            })
        }
        RowValues::Null => rusqlite::types::Value::Null,
        RowValues::JSON(jval) => {
            // Only serialize once to avoid multiple allocations
            let json_str = jval.to_string();
            rusqlite::types::Value::Text(json_str)
        }
        RowValues::Blob(bytes) => {
            if for_execute {
                // For execute, we can directly use the bytes
                rusqlite::types::Value::Blob(bytes.clone())
            } else {
                // For queries, we need to clone
                rusqlite::types::Value::Blob(bytes.clone())
            }
        }
    }
}

/// Bind middleware params to `SQLite` types.
pub fn convert_params(params: &[RowValues]) -> Vec<rusqlite::types::Value> {
    let mut vec_values = Vec::with_capacity(params.len());
    for p in params {
        vec_values.push(row_value_to_sqlite_value(p, false));
    }
    vec_values
}

/// Convert parameters for execution operations
pub fn convert_params_for_execute<I>(iter: I) -> ParamsFromIter<std::vec::IntoIter<Value>>
where
    I: IntoIterator<Item = RowValues>,
{
    // Convert directly to avoid unnecessary collection
    let mut values = Vec::new();
    for p in iter {
        values.push(row_value_to_sqlite_value(&p, true));
    }
    // Note: clippy suggests removing .into_iter(), but removing it causes
    // a type error. This is because params_from_iter needs IntoIter<Value>,
    // not Vec<Value>, despite the type parameter suggesting otherwise.
    #[allow(clippy::useless_conversion)]
    rusqlite::params_from_iter(values.into_iter())
}

/// Wrapper for `SQLite` parameters for queries.
pub struct SqliteParamsQuery(pub Vec<rusqlite::types::Value>);

/// Wrapper for `SQLite` parameters for execution.
pub struct SqliteParamsExecute(
    pub rusqlite::ParamsFromIter<std::vec::IntoIter<rusqlite::types::Value>>,
);

impl ParamConverter<'_> for SqliteParamsQuery {
    type Converted = Self;

    fn convert_sql_params(
        params: &[RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        match mode {
            // For a query, use the conversion that returns a Vec<Value>
            ConversionMode::Query => Ok(SqliteParamsQuery(convert_params(params))),
            // Or, if you really want to support execution mode with this type, you might decide how to handle it:
            ConversionMode::Execute => {
                // For example, you could also call the "query" conversion here or return an error.
                Ok(SqliteParamsQuery(convert_params(params)))
            }
        }
    }

    fn supports_mode(mode: ConversionMode) -> bool {
        // This converter is primarily for query operations
        mode == ConversionMode::Query
    }
}

impl ParamConverter<'_> for SqliteParamsExecute {
    type Converted = Self;

    fn convert_sql_params(
        params: &[RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        match mode {
            ConversionMode::Execute => Ok(SqliteParamsExecute(convert_params_for_execute(
                params.to_vec(),
            ))),
            // For queries you might not support the "execute" wrapper:
            ConversionMode::Query => Err(SqlMiddlewareDbError::ParameterError(
                "SqliteParamsExecute can only be used with Execute mode".into(),
            )),
        }
    }

    fn supports_mode(mode: ConversionMode) -> bool {
        // This converter is only for execution operations
        mode == ConversionMode::Execute
    }
}
