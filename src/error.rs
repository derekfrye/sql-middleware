use thiserror::Error;

#[cfg(any(feature = "postgres", feature = "mssql"))]
use bb8;

#[cfg(feature = "mssql")]
use bb8_tiberius::Error as Bb8TiberiusError;

#[cfg(feature = "sqlite")]
use rusqlite;
#[cfg(feature = "mssql")]
use tiberius;
#[cfg(feature = "postgres")]
use tokio_postgres;
#[cfg(feature = "turso")]
use turso;

#[derive(Debug, Error)]
pub enum SqlMiddlewareDbError {
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    PostgresError(#[from] tokio_postgres::Error),

    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),

    #[cfg(feature = "mssql")]
    #[error(transparent)]
    MssqlError(#[from] tiberius::error::Error),

    #[cfg(feature = "postgres")]
    #[error(transparent)]
    PoolErrorPostgres(#[from] bb8::RunError<tokio_postgres::Error>),

    #[cfg(feature = "mssql")]
    #[error(transparent)]
    PoolErrorMssql(#[from] bb8::RunError<Bb8TiberiusError>),

    #[cfg(feature = "turso")]
    #[error(transparent)]
    TursoError(#[from] turso::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Parameter conversion error: {0}")]
    ParameterError(String),

    #[error("SQL execution error: {0}")]
    ExecutionError(String),

    #[error("Unimplemented feature: {0}")]
    Unimplemented(String),

    #[error("Other database error: {0}")]
    Other(String),
}

#[cfg(feature = "sqlite")]
impl From<bb8::RunError<SqlMiddlewareDbError>> for SqlMiddlewareDbError {
    fn from(err: bb8::RunError<SqlMiddlewareDbError>) -> Self {
        SqlMiddlewareDbError::ConnectionError(format!("SQLite pool error: {err}"))
    }
}
