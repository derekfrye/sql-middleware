//! Convenient imports for common functionality.
//!
//! This module re-exports the most commonly used types and functions
//! to make it easier to get started with the library.

pub use crate::middleware::{
    AnyConnWrapper, AsyncDatabaseExecutor, ConfigAndPool, ConversionMode, CustomDbRow,
    DatabaseType, MiddlewarePool, MiddlewarePoolConnection, QueryAndParams, ResultSet,
    RowValues, SqlMiddlewareDbError,
};

pub use crate::conversion::convert_sql_params;

#[cfg(feature = "postgres")]
pub use crate::exports::PostgresParams;
#[cfg(feature = "postgres")]
pub use crate::exports::postgres_build_result_set;

#[cfg(feature = "sqlite")]
pub use crate::exports::SqliteParamsExecute;
#[cfg(feature = "sqlite")]
pub use crate::exports::SqliteParamsQuery;
#[cfg(feature = "sqlite")]
pub use crate::exports::sqlite_build_result_set;

#[cfg(feature = "mssql")]
pub use crate::exports::MssqlClient;
#[cfg(feature = "mssql")]
pub use crate::exports::MssqlParams;
#[cfg(feature = "mssql")]
pub use crate::exports::create_mssql_client;
#[cfg(feature = "mssql")]
pub use crate::exports::mssql_build_result_set;