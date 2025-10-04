#[cfg(feature = "postgres")]
use deadpool_postgres::Object as PostgresObject;

#[cfg(feature = "sqlite")]
use crate::sqlite::{SqliteConnection, SqlitePreparedStatement};
#[cfg(feature = "sqlite")]
use deadpool_sqlite::Object as SqliteObject;
#[cfg(feature = "sqlite")]
use deadpool_sqlite::rusqlite;

#[cfg(feature = "libsql")]
use deadpool_libsql::Object as LibsqlObject;
#[cfg(feature = "turso")]
use turso::Connection as TursoConnection;

use super::types::MiddlewarePool;
use crate::error::SqlMiddlewareDbError;

pub enum MiddlewarePoolConnection {
    #[cfg(feature = "postgres")]
    Postgres(PostgresObject),
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteConnection),
    #[cfg(feature = "mssql")]
    Mssql(deadpool::managed::Object<deadpool_tiberius::Manager>),
    #[cfg(feature = "libsql")]
    Libsql(LibsqlObject),
    #[cfg(feature = "turso")]
    Turso(TursoConnection),
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
            #[cfg(feature = "turso")]
            Self::Turso(_) => f.debug_tuple("Turso").field(&"<Connection>").finish(),
        }
    }
}

impl MiddlewarePool {
    /// Get a connection from the pool
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::PoolErrorPostgres` or `SqlMiddlewareDbError::PoolErrorSqlite` if the pool fails to provide a connection.
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
                let worker_conn = SqliteConnection::new(conn)?;
                Ok(MiddlewarePoolConnection::Sqlite(worker_conn))
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
            #[cfg(feature = "turso")]
            MiddlewarePool::Turso(db) => {
                let conn: TursoConnection = db.connect().map_err(SqlMiddlewareDbError::from)?;
                Ok(MiddlewarePoolConnection::Turso(conn))
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }
}

impl MiddlewarePoolConnection {
    /// Run synchronous SQLite work on the underlying worker-owned connection.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError::Unimplemented`] when the connection is not SQLite.
    #[cfg(feature = "sqlite")]
    pub async fn with_sqlite_connection<F, R>(&mut self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        match self {
            MiddlewarePoolConnection::Sqlite(sqlite_conn) => {
                sqlite_conn.with_connection(func).await
            }
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "with_sqlite_connection is only available for SQLite connections".to_string(),
            )),
        }
    }

    /// Prepare a SQLite statement and obtain a reusable handle backed by the worker thread.
    #[cfg(feature = "sqlite")]
    pub async fn prepare_sqlite_statement(
        &mut self,
        query: &str,
    ) -> Result<SqlitePreparedStatement, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Sqlite(sqlite_conn) => {
                sqlite_conn.prepare_statement(query).await
            }
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "prepare_sqlite_statement is only available for SQLite connections".to_string(),
            )),
        }
    }
}
