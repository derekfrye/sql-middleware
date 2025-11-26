use chrono::NaiveDateTime;
use clap::ValueEnum;
use serde_json::Value as JsonValue;

use crate::error::SqlMiddlewareDbError;

/// Values that can be stored in a database row or used as query parameters.
///
/// Reuse the same enum across backends so helper functions do not need to branch on driver
/// types:
/// ```rust
/// use sql_middleware::prelude::*;
///
/// let params = vec![
///     RowValues::Int(1),
///     RowValues::Text("alice".into()),
///     RowValues::Bool(true),
/// ];
/// # let _ = params;
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum RowValues {
    /// Integer value (64-bit)
    Int(i64),
    /// Floating point value (64-bit)
    Float(f64),
    /// Text/string value
    Text(String),
    /// Boolean value
    Bool(bool),
    /// Timestamp value
    Timestamp(NaiveDateTime),
    /// NULL value
    Null,
    /// JSON value
    JSON(JsonValue),
    /// Binary data
    Blob(Vec<u8>),
}

impl RowValues {
    /// Check if this value is NULL
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    #[must_use]
    pub fn as_int(&self) -> Option<&i64> {
        if let RowValues::Int(value) = self {
            Some(value)
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        if let RowValues::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<&bool> {
        if let RowValues::Bool(value) = self {
            return Some(value);
        } else if let Some(i) = self.as_int() {
            if *i == 1 {
                return Some(&true);
            } else if *i == 0 {
                return Some(&false);
            }
        }
        None
    }

    #[must_use]
    pub fn as_timestamp(&self) -> Option<chrono::NaiveDateTime> {
        if let RowValues::Timestamp(value) = self {
            return Some(*value);
        } else if let Some(s) = self.as_text() {
            // Try "YYYY-MM-DD HH:MM:SS"
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
                return Some(dt);
            }
            // Try "YYYY-MM-DD HH:MM:SS.SSS"
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S.%3f") {
                return Some(dt);
            }
        }
        None
    }

    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        if let RowValues::Float(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_blob(&self) -> Option<&[u8]> {
        if let RowValues::Blob(bytes) = self {
            Some(bytes)
        } else {
            None
        }
    }
}

/// The database type supported by this middleware
#[derive(Debug, Clone, PartialEq, Eq, Hash, ValueEnum)]
pub enum DatabaseType {
    /// `PostgreSQL` database
    #[cfg(feature = "postgres")]
    Postgres,
    /// `SQLite` database
    #[cfg(feature = "sqlite")]
    Sqlite,
    /// SQL Server database
    #[cfg(feature = "mssql")]
    Mssql,
    /// `LibSQL` database
    #[cfg(feature = "libsql")]
    Libsql,
    /// Turso (SQLite-compatible, in-process) database
    #[cfg(feature = "turso")]
    Turso,
}

/// The conversion "mode".
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConversionMode {
    /// When the converted parameters will be used in a query (SELECT)
    Query,
    /// When the converted parameters will be used for statement execution (INSERT/UPDATE/etc.)
    Execute,
}

/// Convert a slice of `RowValues` into database-specific parameters.
/// This trait provides a unified interface for converting generic `RowValues`
/// to database-specific parameter types.
pub trait ParamConverter<'a> {
    type Converted;

    /// Convert a slice of `RowValues` into the backend's parameter type.
    ///
    /// # Errors
    ///
    /// Returns `SqlMiddlewareDbError` if the conversion fails for any parameter.
    fn convert_sql_params(
        params: &'a [RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError>;

    /// Check if this converter supports the given mode
    ///
    /// # Arguments
    /// * `mode` - The conversion mode to check
    ///
    /// # Returns
    /// * `bool` - Whether this converter supports the mode
    #[must_use]
    fn supports_mode(_mode: ConversionMode) -> bool {
        true // By default, support both modes
    }
}
