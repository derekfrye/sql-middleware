//! Convenient imports for common functionality.
//!
//! This module re-exports the most commonly used types and functions
//! to make it easier to get started with the library.

pub use crate::middleware::{
    AnyConnWrapper, BatchTarget, ConfigAndPool, ConversionMode, CustomDbRow, DatabaseType,
    MiddlewarePool, MiddlewarePoolConnection, QueryAndParams, QueryBuilder, QueryTarget, ResultSet,
    RowValues, SqlMiddlewareDbError, TxOutcome, execute_batch, query,
};

pub use crate::conversion::convert_sql_params;
#[cfg(feature = "libsql")]
pub use crate::libsql::{
    LibsqlOptions, LibsqlOptionsBuilder, LibsqlRemoteOptions, LibsqlRemoteOptionsBuilder,
};
#[cfg(feature = "mssql")]
pub use crate::mssql::{MssqlOptions, MssqlOptionsBuilder};
#[cfg(feature = "postgres")]
pub use crate::postgres::{PgConfig, PostgresOptions, PostgresOptionsBuilder};
#[cfg(feature = "sqlite")]
pub use crate::sqlite::{SqliteOptions, SqliteOptionsBuilder};
pub use crate::translation::{
    PlaceholderStyle, QueryOptions, TranslationMode, translate_placeholders,
};
#[cfg(feature = "turso")]
pub use crate::turso::{TursoOptions, TursoOptionsBuilder};
