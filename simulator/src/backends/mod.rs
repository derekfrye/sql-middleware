use async_trait::async_trait;
use sql_middleware::middleware::MiddlewarePoolConnection;
use sql_middleware::{ResultSet, SqlMiddlewareDbError};

use crate::args::{BackendKind, SimConfig};

#[cfg(feature = "postgres")]
pub(crate) mod postgres;
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;
#[cfg(feature = "turso")]
pub(crate) mod turso;

#[derive(Debug)]
pub(crate) enum BackendError {
    Init(String),
    Sql(SqlMiddlewareDbError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ErrorClass {
    Init,
    Sql(SqlErrorClass),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlErrorClass {
    Config,
    Connection,
    Parameter,
    Execution,
    Unimplemented,
    Other,
    #[cfg(feature = "postgres")]
    Postgres,
    #[cfg(feature = "sqlite")]
    Sqlite,
    #[cfg(feature = "mssql")]
    Mssql,
    #[cfg(feature = "postgres")]
    PoolPostgres,
    #[cfg(feature = "mssql")]
    PoolMssql,
    #[cfg(feature = "turso")]
    Turso,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Init(message) => write!(f, "{message}"),
            BackendError::Sql(err) => write!(f, "{err}"),
        }
    }
}

impl From<SqlMiddlewareDbError> for BackendError {
    fn from(err: SqlMiddlewareDbError) -> Self {
        BackendError::Sql(err)
    }
}

impl BackendError {
    pub(crate) fn class(&self) -> ErrorClass {
        match self {
            BackendError::Init(_) => ErrorClass::Init,
            BackendError::Sql(err) => ErrorClass::Sql(classify_sql_error(err)),
        }
    }
}

fn classify_sql_error(err: &SqlMiddlewareDbError) -> SqlErrorClass {
    match err {
        #[cfg(feature = "postgres")]
        SqlMiddlewareDbError::PostgresError(_) => SqlErrorClass::Postgres,
        #[cfg(feature = "sqlite")]
        SqlMiddlewareDbError::SqliteError(_) => SqlErrorClass::Sqlite,
        #[cfg(feature = "mssql")]
        SqlMiddlewareDbError::MssqlError(_) => SqlErrorClass::Mssql,
        #[cfg(feature = "postgres")]
        SqlMiddlewareDbError::PoolErrorPostgres(_) => SqlErrorClass::PoolPostgres,
        #[cfg(feature = "mssql")]
        SqlMiddlewareDbError::PoolErrorMssql(_) => SqlErrorClass::PoolMssql,
        #[cfg(feature = "turso")]
        SqlMiddlewareDbError::TursoError(_) => SqlErrorClass::Turso,
        SqlMiddlewareDbError::ConfigError(_) => SqlErrorClass::Config,
        SqlMiddlewareDbError::ConnectionError(_) => SqlErrorClass::Connection,
        SqlMiddlewareDbError::ParameterError(_) => SqlErrorClass::Parameter,
        SqlMiddlewareDbError::ExecutionError(_) => SqlErrorClass::Execution,
        SqlMiddlewareDbError::Unimplemented(_) => SqlErrorClass::Unimplemented,
        SqlMiddlewareDbError::Other(_) => SqlErrorClass::Other,
    }
}

#[async_trait]
pub(crate) trait Backend: Send + Sync {
    async fn checkout(&self) -> Result<MiddlewarePoolConnection, BackendError>;
    async fn begin(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError>;
    async fn commit(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError>;
    async fn rollback(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError>;
    async fn execute(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<(), BackendError>;
    async fn query(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<ResultSet, BackendError>;
    async fn sleep(&self, ms: u64);
}

pub(crate) async fn build_backend(
    kind: BackendKind,
    pool_size: usize,
    config: &SimConfig,
) -> Result<Box<dyn Backend>, BackendError> {
    match kind {
        BackendKind::Sqlite => {
            #[cfg(feature = "sqlite")]
            {
                let backend =
                    sqlite::SqliteBackend::new(sqlite::SqliteBackendConfig::in_memory(pool_size))
                        .await?;
                Ok(Box::new(backend))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = (pool_size, config);
                Err(BackendError::Init(
                    "sqlite backend not enabled; build with --features sqlite".to_string(),
                ))
            }
        }
        BackendKind::Postgres => {
            #[cfg(feature = "postgres")]
            {
                let backend = postgres::PostgresBackend::new(config).await?;
                Ok(Box::new(backend))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = (pool_size, config);
                Err(BackendError::Init(
                    "postgres backend not enabled; build with --features postgres".to_string(),
                ))
            }
        }
        BackendKind::Turso => {
            #[cfg(feature = "turso")]
            {
                let backend = turso::TursoBackend::new(config).await?;
                Ok(Box::new(backend))
            }
            #[cfg(not(feature = "turso"))]
            {
                let _ = (pool_size, config);
                Err(BackendError::Init(
                    "turso backend not enabled; build with --features turso".to_string(),
                ))
            }
        }
    }
}
