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
pub mod prelude;
pub mod translation;
#[cfg(feature = "typed-postgres")]
pub mod typed_postgres;
#[cfg(feature = "typed-turso")]
pub mod typed_turso;
pub mod typed_api;

// Core modules (public for docs/advanced use)
pub mod error;
pub(crate) mod executor;
pub mod middleware;
pub mod pool;
pub mod query;

// Internal modules (types are re-exported; modules stay private)
pub(crate) mod exports;
pub(crate) mod helpers;
pub(crate) mod query_builder;
pub(crate) mod results;
pub(crate) mod types;

// Private database-specific modules
#[cfg(feature = "libsql")]
pub mod libsql;
#[cfg(feature = "mssql")]
pub mod mssql;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "turso")]
pub mod turso;

// Direct exports for frequently used types
pub use middleware::{
    AnyConnWrapper, BatchTarget, ConfigAndPool, ConversionMode, CustomDbRow, DatabaseType,
    MiddlewarePool, MiddlewarePoolConnection, ParamConverter, QueryAndParams, QueryBuilder,
    QueryTarget, ResultSet, RowValues, SqlMiddlewareDbError, execute_batch,
};
#[cfg(feature = "libsql")]
pub use middleware::{
    LibsqlOptions, LibsqlOptionsBuilder, LibsqlRemoteOptions, LibsqlRemoteOptionsBuilder,
};
#[cfg(feature = "mssql")]
pub use middleware::{MssqlOptions, MssqlOptionsBuilder};
#[cfg(feature = "postgres")]
pub use middleware::{PostgresOptions, PostgresOptionsBuilder};
#[cfg(feature = "sqlite")]
pub use middleware::{SqliteOptions, SqliteOptionsBuilder};
#[cfg(feature = "turso")]
pub use middleware::{TursoOptions, TursoOptionsBuilder};

// Re-export from modules for convenience
pub use conversion::convert_sql_params;
pub use translation::{PlaceholderStyle, QueryOptions, TranslationMode, translate_placeholders};

// Compatibility alias for existing code
pub mod test_helpers {
    pub use crate::helpers::create_test_row;
}
