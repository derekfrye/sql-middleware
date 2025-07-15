//! Database-specific type exports.
//!
//! This module contains all the conditional feature exports for different
//! database backends, keeping them organized in one place.

// PostgreSQL exports
#[cfg(feature = "postgres")]
pub use crate::postgres::Params as PostgresParams;
#[cfg(feature = "postgres")]
pub use crate::postgres::build_result_set as postgres_build_result_set;

// SQLite exports
#[cfg(feature = "sqlite")]
pub use crate::sqlite::SqliteParamsExecute;
#[cfg(feature = "sqlite")]
pub use crate::sqlite::SqliteParamsQuery;
#[cfg(feature = "sqlite")]
pub use crate::sqlite::build_result_set as sqlite_build_result_set;

// SQL Server exports
#[cfg(feature = "mssql")]
pub use crate::mssql::MssqlClient;
#[cfg(feature = "mssql")]
pub use crate::mssql::Params as MssqlParams;
#[cfg(feature = "mssql")]
pub use crate::mssql::build_result_set as mssql_build_result_set;
#[cfg(feature = "mssql")]
pub use crate::mssql::create_mssql_client;