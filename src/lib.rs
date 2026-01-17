#![doc = include_str!("../docs.md")]
#![forbid(unsafe_code)]

// Test utilities module
#[path = "test_utils/test_helpers.rs"]
pub mod test_helpers;

// Benchmark utilities module - for benchmarks
#[cfg(feature = "benchmarks")]
pub mod benchmark;

// Public API modules
pub mod conversion;
pub mod prelude;
pub mod translation;
pub mod tx_outcome;
pub mod typed;
/// Back-compat re-export: `typed_api` is now `typed`.
pub use typed as typed_api;
#[cfg(feature = "postgres")]
pub mod typed_postgres;
#[cfg(feature = "sqlite")]
pub mod typed_sqlite;
#[cfg(feature = "turso")]
pub mod typed_turso;

// Core modules (public for docs/advanced use)
pub mod error;
pub(crate) mod executor;
pub mod middleware;
pub mod pool;
pub mod query;

// Internal modules (types are re-exported; modules stay private)
pub(crate) mod query_builder;
pub(crate) mod results;
pub(crate) mod types;

// Private database-specific modules
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
    QueryTarget, ResultSet, RowValues, SqlMiddlewareDbError, TxOutcome, execute_batch,
};
#[cfg(feature = "mssql")]
pub use middleware::{MssqlOptions, MssqlOptionsBuilder};
#[cfg(feature = "postgres")]
pub use middleware::{PgConfig, PostgresOptions, PostgresOptionsBuilder};
#[cfg(feature = "sqlite")]
pub use middleware::{SqliteOptions, SqliteOptionsBuilder};
#[cfg(feature = "turso")]
pub use middleware::{TursoOptions, TursoOptionsBuilder};

// Re-export from modules for convenience
pub use conversion::convert_sql_params;
pub use translation::{
    PlaceholderStyle, PrepareMode, QueryOptions, TranslationMode, translate_placeholders,
};
