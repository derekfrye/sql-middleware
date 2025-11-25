#![doc = include_str!("../docs.md")]
#![forbid(unsafe_code)]

// Test utilities module - only compiled with test-utils feature
#[cfg(feature = "test-utils")]
pub mod test_utils;

// Benchmark utilities module - for benchmarks
#[cfg(feature = "benchmarks")]
pub mod benchmark;

// Public API modules
pub mod conversion;
pub mod exports;
pub mod helpers;
pub mod prelude;
pub mod query_builder;
pub mod translation;

// Core modules
pub mod error;
pub mod executor;
pub mod middleware;
pub mod pool;
pub mod query;
pub mod results;
pub mod types;

// Private database-specific modules
#[cfg(feature = "libsql")]
pub mod libsql;
#[cfg(feature = "mssql")]
mod mssql;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "turso")]
pub mod turso;

#[cfg(feature = "benchmarks")]
pub use crate::sqlite::{
    params::{SqliteParamsExecute, SqliteParamsQuery},
    query::build_result_set as sqlite_build_result_set,
};

#[cfg(feature = "benchmarks")]
pub use crate::conversion::convert_sql_params as sqlite_convert_params;

// Direct exports for frequently used types
pub use middleware::{
    AnyConnWrapper, ConfigAndPool, ConversionMode, CustomDbRow, DatabaseType, MiddlewarePool,
    MiddlewarePoolConnection, ParamConverter, QueryAndParams, QueryBuilder, ResultSet, RowValues,
    SqlMiddlewareDbError,
};

// Re-export from modules for convenience
pub use conversion::convert_sql_params;
pub use exports::*;
pub use translation::{PlaceholderStyle, QueryOptions, TranslationMode, translate_placeholders};

// Compatibility alias for existing code
pub mod test_helpers {
    pub use crate::helpers::create_test_row;
}
