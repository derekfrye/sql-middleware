// Re-export all the types and traits from the sub-modules
pub use crate::error::SqlMiddlewareDbError;
pub use crate::executor::{BatchTarget, QueryTarget, execute_batch, query};
pub use crate::pool::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
pub use crate::query::{AnyConnWrapper, QueryAndParams};
pub use crate::query_builder::QueryBuilder;
pub use crate::results::{CustomDbRow, ResultSet};
pub use crate::translation::{
    PlaceholderStyle, QueryOptions, TranslationMode, translate_placeholders,
};
pub use crate::types::{ConversionMode, DatabaseType, ParamConverter, RowValues};

#[cfg(feature = "libsql")]
pub use crate::libsql::{
    LibsqlOptions, LibsqlOptionsBuilder, LibsqlRemoteOptions, LibsqlRemoteOptionsBuilder,
};
#[cfg(feature = "mssql")]
pub use crate::mssql::{MssqlOptions, MssqlOptionsBuilder};
#[cfg(feature = "postgres")]
pub use crate::postgres::{PostgresOptions, PostgresOptionsBuilder};
#[cfg(feature = "sqlite")]
pub use crate::sqlite::{SqliteOptions, SqliteOptionsBuilder};
#[cfg(feature = "turso")]
pub use crate::turso::{TursoOptions, TursoOptionsBuilder};
