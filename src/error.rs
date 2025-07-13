use thiserror::Error;

#[cfg(feature = "postgres")]
use tokio_postgres;
#[cfg(feature = "sqlite")]
use deadpool_sqlite::rusqlite;
#[cfg(feature = "mssql")]
use tiberius;

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
    PoolErrorPostgres(#[from] deadpool::managed::PoolError<tokio_postgres::Error>),
    
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    PoolErrorSqlite(#[from] deadpool::managed::PoolError<rusqlite::Error>),
    
    #[cfg(feature = "mssql")]
    #[error(transparent)]
    PoolErrorMssql(#[from] deadpool::managed::PoolError<tiberius::error::Error>),
    
    #[cfg(feature = "mssql")]
    #[error("SQL Server connection pool error: {0}")]
    TiberiusPoolError(String),
    
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