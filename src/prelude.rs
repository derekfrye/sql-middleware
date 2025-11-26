//! Convenient imports for common functionality.
//!
//! This module re-exports the most commonly used types and functions
//! to make it easier to get started with the library.

pub use crate::middleware::{
    AnyConnWrapper, ConfigAndPool, ConversionMode, CustomDbRow, DatabaseType, MiddlewarePool,
    MiddlewarePoolConnection, QueryAndParams, QueryBuilder, ResultSet, RowValues,
    SqlMiddlewareDbError,
};

pub use crate::conversion::convert_sql_params;
pub use crate::translation::{
    PlaceholderStyle, QueryOptions, TranslationMode, translate_placeholders,
};

