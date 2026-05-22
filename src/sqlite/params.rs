use std::fmt::Write;

use rusqlite;

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

/// Reusable SQLite parameter buffer for hot prepared-statement loops.
///
/// Callers can allocate once, mutate values in place with the `set_*` methods, and pass the
/// buffer to `SqlitePreparedStatement::*_params` methods without rebuilding driver values from
/// `RowValues` on every iteration.
#[derive(Debug, Clone, Default)]
pub struct SqliteParamsBuf {
    values: Vec<rusqlite::types::Value>,
}

impl SqliteParamsBuf {
    /// Create an empty buffer with room for `capacity` parameters.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
        }
    }

    /// Remove all values while keeping allocated capacity.
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Number of parameters currently in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the buffer has no parameters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Borrow the driver-native values.
    #[must_use]
    pub fn as_values(&self) -> &[rusqlite::types::Value] {
        &self.values
    }

    /// Set an integer parameter at zero-based `index`.
    pub fn set_int(&mut self, index: usize, value: i64) {
        self.set_value(index, rusqlite::types::Value::Integer(value));
    }

    /// Set a floating-point parameter at zero-based `index`.
    pub fn set_float(&mut self, index: usize, value: f64) {
        self.set_value(index, rusqlite::types::Value::Real(value));
    }

    /// Set a text parameter at zero-based `index`.
    pub fn set_text(&mut self, index: usize, value: impl Into<String>) {
        self.set_value(index, rusqlite::types::Value::Text(value.into()));
    }

    /// Set a boolean parameter at zero-based `index`.
    pub fn set_bool(&mut self, index: usize, value: bool) {
        self.set_value(index, rusqlite::types::Value::Integer(i64::from(value)));
    }

    /// Set a timestamp parameter at zero-based `index`.
    pub fn set_timestamp(&mut self, index: usize, value: chrono::NaiveDateTime) {
        self.set_value(
            index,
            row_value_to_sqlite_value(&RowValues::Timestamp(value), true),
        );
    }

    /// Set a JSON parameter at zero-based `index`.
    pub fn set_json(&mut self, index: usize, value: serde_json::Value) {
        self.set_value(index, rusqlite::types::Value::Text(value.to_string()));
    }

    /// Set a blob parameter at zero-based `index`.
    pub fn set_blob(&mut self, index: usize, value: impl Into<Vec<u8>>) {
        self.set_value(index, rusqlite::types::Value::Blob(value.into()));
    }

    /// Set a NULL parameter at zero-based `index`.
    pub fn set_null(&mut self, index: usize) {
        self.set_value(index, rusqlite::types::Value::Null);
    }

    fn set_value(&mut self, index: usize, value: rusqlite::types::Value) {
        if self.values.len() <= index {
            self.values
                .resize_with(index + 1, || rusqlite::types::Value::Null);
        }
        self.values[index] = value;
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
