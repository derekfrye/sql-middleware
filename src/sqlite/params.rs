use deadpool_sqlite::rusqlite;
use std::fmt::Write;

use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

// Thread-local buffer for efficient timestamp formatting
thread_local! {
    static TIMESTAMP_BUF: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(32));
}

/// Convert a single `RowValue` to a rusqlite `Value`.
#[must_use]
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
                rusqlite::types::Value::Blob(bytes.clone())
            }
        }
    }
}

/// Unified `SQLite` parameter container.
pub struct Params(pub Vec<rusqlite::types::Value>);

impl Params {
    /// Convert middleware row values into `SQLite` values.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError::ConversionError` if parameter conversion fails.
    pub fn convert(params: &[RowValues]) -> Result<Self, SqlMiddlewareDbError> {
        let mut vec_values = Vec::with_capacity(params.len());
        for p in params {
            vec_values.push(row_value_to_sqlite_value(p, true));
        }
        Ok(Params(vec_values))
    }

    /// Borrow the underlying values.
    #[must_use]
    pub fn as_values(&self) -> &[rusqlite::types::Value] {
        &self.0
    }

    /// Build a borrowed params slice suitable for rusqlite execution.
    #[must_use]
    pub fn as_refs(&self) -> Vec<&dyn rusqlite::ToSql> {
        self.0.iter().map(|v| v as &dyn rusqlite::ToSql).collect()
    }
}

impl ParamConverter<'_> for Params {
    type Converted = Params;

    fn convert_sql_params(
        params: &[RowValues],
        _mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        Self::convert(params)
    }

    fn supports_mode(mode: ConversionMode) -> bool {
        // Single Params type supports both query and execute.
        matches!(mode, ConversionMode::Query | ConversionMode::Execute)
    }
}
