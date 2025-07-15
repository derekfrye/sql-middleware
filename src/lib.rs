#![doc = include_str!("../docs.md")]
#![forbid(unsafe_code)]

// Test utilities module - only compiled with test-utils feature
#[cfg(feature = "test-utils")]
pub mod test_utils;

// Public API modules
pub mod conversion;
pub mod exports;
pub mod helpers;
pub mod prelude;

// Core modules
pub mod error;
pub mod executor;
pub mod middleware;
pub mod pool;
pub mod query;
pub mod results;
pub mod types;

// Private database-specific modules
#[cfg(feature = "mssql")]
mod mssql;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

// Direct exports for frequently used types
pub use middleware::{
    AnyConnWrapper, AsyncDatabaseExecutor, ConfigAndPool, ConversionMode, CustomDbRow,
    DatabaseType, MiddlewarePool, MiddlewarePoolConnection, ParamConverter, QueryAndParams,
    ResultSet, RowValues, SqlMiddlewareDbError,
};

// Re-export from modules for convenience
pub use conversion::convert_sql_params;
pub use exports::*;

// Compatibility alias for existing code
pub mod test_helpers {
    pub use crate::helpers::create_test_row;
}
