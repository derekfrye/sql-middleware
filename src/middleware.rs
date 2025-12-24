// Re-export all the types and traits from the sub-modules
pub use crate::error::SqlMiddlewareDbError;
pub use crate::executor::{BatchTarget, QueryTarget, execute_batch, query};
pub use crate::pool::{AnyConnWrapper, ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
pub use crate::query::QueryAndParams;
pub use crate::query_builder::QueryBuilder;
pub use crate::results::{CustomDbRow, ResultSet};
pub use crate::translation::{
    PlaceholderStyle, QueryOptions, TranslationMode, translate_placeholders,
};
pub use crate::tx_outcome::TxOutcome;
pub use crate::types::{ConversionMode, DatabaseType, ParamConverter, RowValues};

#[cfg(feature = "mssql")]
pub use crate::mssql::{MssqlOptions, MssqlOptionsBuilder};
#[cfg(feature = "postgres")]
pub use crate::postgres::{PgConfig, PostgresOptions, PostgresOptionsBuilder};
#[cfg(feature = "sqlite")]
pub use crate::sqlite::{SqliteOptions, SqliteOptionsBuilder};
#[cfg(feature = "turso")]
pub use crate::turso::{TursoOptions, TursoOptionsBuilder};
