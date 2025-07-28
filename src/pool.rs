#[cfg(feature = "postgres")]
use deadpool_postgres::{Object as PostgresObject, Pool as DeadpoolPostgresPool};

#[cfg(feature = "sqlite")]
use deadpool_sqlite::{Object as SqliteObject, Pool as DeadpoolSqlitePool};

#[cfg(feature = "sqlite")]
pub type SqliteWritePool = DeadpoolSqlitePool;

#[cfg(feature = "mssql")]
use deadpool_tiberius::Pool as TiberiusPool;

#[cfg(feature = "libsql")]
use deadpool_libsql::{Object as LibsqlObject, Pool as DeadpoolLibsqlPool};

use crate::error::SqlMiddlewareDbError;
use crate::query::AnyConnWrapper;
use crate::types::DatabaseType;

/// Connection pool for database access
///
/// This enum wraps the different connection pool types for the
/// supported database engines.
#[derive(Clone)]
pub enum MiddlewarePool {
    /// `PostgreSQL` connection pool
    #[cfg(feature = "postgres")]
    Postgres(DeadpoolPostgresPool),
    /// `SQLite` connection pool
    #[cfg(feature = "sqlite")]
    Sqlite(DeadpoolSqlitePool),
    /// SQL Server connection pool
    #[cfg(feature = "mssql")]
    Mssql(TiberiusPool),
    /// `LibSQL` connection pool
    #[cfg(feature = "libsql")]
    Libsql(DeadpoolLibsqlPool),
}

// Manual Debug implementation because deadpool_tiberius::Manager doesn't implement Debug
impl std::fmt::Debug for MiddlewarePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(pool) => f.debug_tuple("Postgres").field(pool).finish(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(pool) => f.debug_tuple("Sqlite").field(pool).finish(),
            #[cfg(feature = "mssql")]
            Self::Mssql(_) => f.debug_tuple("Mssql").field(&"<TiberiusPool>").finish(),
            #[cfg(feature = "libsql")]
            Self::Libsql(pool) => f.debug_tuple("Libsql").field(pool).finish(),
        }
    }
}

/// Configuration and connection pool for a database
///
/// This struct holds both the configuration and the connection pool
/// for a database, making it easier to manage database connections.
#[derive(Clone, Debug)]
pub struct ConfigAndPool {
    /// The connection pool
    pub pool: MiddlewarePool,
    /// The database type
    pub db_type: DatabaseType,
}

impl MiddlewarePool {
    // Return a reference to self instead of cloning the entire pool  
    #[allow(clippy::unused_async)]
    pub async fn get(&self) -> Result<&MiddlewarePool, SqlMiddlewareDbError> {
        Ok(self)
    }

    pub async fn get_connection(
        pool: &MiddlewarePool,
    ) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
        match pool {
            #[cfg(feature = "postgres")]
            MiddlewarePool::Postgres(pool) => {
                let conn: PostgresObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorPostgres)?;
                Ok(MiddlewarePoolConnection::Postgres(conn))
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePool::Sqlite(pool) => {
                let conn: SqliteObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorSqlite)?;
                Ok(MiddlewarePoolConnection::Sqlite(conn))
            }
            #[cfg(feature = "mssql")]
            MiddlewarePool::Mssql(pool) => {
                let conn = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorMssql)?;
                Ok(MiddlewarePoolConnection::Mssql(conn))
            }
            #[cfg(feature = "libsql")]
            MiddlewarePool::Libsql(pool) => {
                let conn: LibsqlObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorLibsql)?;
                Ok(MiddlewarePoolConnection::Libsql(conn))
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }
}

pub enum MiddlewarePoolConnection {
    #[cfg(feature = "postgres")]
    Postgres(PostgresObject),
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteObject),
    #[cfg(feature = "mssql")]
    Mssql(deadpool::managed::Object<deadpool_tiberius::Manager>),
    #[cfg(feature = "libsql")]
    Libsql(LibsqlObject),
}

// Manual Debug implementation because deadpool_tiberius::Manager doesn't implement Debug
impl std::fmt::Debug for MiddlewarePoolConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(conn) => f.debug_tuple("Postgres").field(conn).finish(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(conn) => f.debug_tuple("Sqlite").field(conn).finish(),
            #[cfg(feature = "mssql")]
            Self::Mssql(_) => f
                .debug_tuple("Mssql")
                .field(&"<TiberiusConnection>")
                .finish(),
            #[cfg(feature = "libsql")]
            Self::Libsql(conn) => f.debug_tuple("Libsql").field(conn).finish(),
        }
    }
}

impl MiddlewarePoolConnection {
    #[allow(unused_variables)]
    pub async fn interact_async<F, Fut>(
        &mut self,
        func: F,
    ) -> Result<Fut::Output, SqlMiddlewareDbError>
    where
        F: FnOnce(AnyConnWrapper<'_>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), SqlMiddlewareDbError>> + Send + 'static,
    {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_obj) => {
                // Assuming PostgresObject dereferences to tokio_postgres::Client
                let client: &mut tokio_postgres::Client = pg_obj.as_mut();
                Ok(func(AnyConnWrapper::Postgres(client)).await)
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_obj) => {
                // Get client from Object
                let client = &mut **mssql_obj;
                Ok(func(AnyConnWrapper::Mssql(client)).await)
            }
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql(libsql_obj) => {
                Ok(func(AnyConnWrapper::Libsql(libsql_obj)).await)
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(_) => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_async is not supported for SQLite; use interact_sync instead".to_string(),
            )),
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_async is not implemented for this database type".to_string(),
            )),
        }
    }

    #[allow(unused_variables)]
    pub async fn interact_sync<F, R>(&self, f: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(AnyConnWrapper) -> R + Send + 'static,
        R: Send + 'static,
    {
        match self {
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_obj) => {
                // Use `deadpool_sqlite`'s `interact` method
                sqlite_obj
                    .interact(move |conn| {
                        let wrapper = AnyConnWrapper::Sqlite(conn);
                        Ok(f(wrapper))
                    })
                    .await?
            }
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(_) => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not supported for Postgres; use interact_async instead"
                    .to_string(),
            )),
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(_) => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not supported for SQL Server; use interact_async instead"
                    .to_string(),
            )),
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not implemented for this database type".to_string(),
            )),
        }
    }
}
