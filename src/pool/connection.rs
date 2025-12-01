#[cfg(feature = "postgres")]
use deadpool_postgres::Object as PostgresObject;

#[cfg(feature = "sqlite")]
use crate::sqlite::{SqliteConnection, SqlitePreparedStatement};
#[cfg(feature = "sqlite")]
use deadpool_sqlite::Object as SqliteObject;
#[cfg(feature = "sqlite")]
use deadpool_sqlite::rusqlite;

#[cfg(feature = "turso")]
use crate::turso::TursoNonTxPreparedStatement;
#[cfg(feature = "libsql")]
use deadpool_libsql::Object as LibsqlObject;
#[cfg(feature = "turso")]
use turso::Connection as TursoConnection;

use super::types::MiddlewarePool;
use crate::error::SqlMiddlewareDbError;

pub enum MiddlewarePoolConnection {
    #[cfg(feature = "postgres")]
    Postgres {
        client: PostgresObject,
        translate_placeholders: bool,
    },
    #[cfg(feature = "sqlite")]
    Sqlite {
        conn: SqliteConnection,
        translate_placeholders: bool,
    },
    #[cfg(feature = "mssql")]
    Mssql {
        conn: deadpool::managed::Object<deadpool_tiberius::Manager>,
        translate_placeholders: bool,
    },
    #[cfg(feature = "libsql")]
    Libsql {
        conn: LibsqlObject,
        translate_placeholders: bool,
    },
    #[cfg(feature = "turso")]
    Turso {
        conn: TursoConnection,
        translate_placeholders: bool,
    },
}

// Manual Debug implementation because deadpool_tiberius::Manager doesn't implement Debug
impl std::fmt::Debug for MiddlewarePoolConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres { client, .. } => f.debug_tuple("Postgres").field(client).finish(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite { conn, .. } => f.debug_tuple("Sqlite").field(conn).finish(),
            #[cfg(feature = "mssql")]
            Self::Mssql { .. } => f
                .debug_tuple("Mssql")
                .field(&"<TiberiusConnection>")
                .finish(),
            #[cfg(feature = "libsql")]
            Self::Libsql { conn, .. } => f.debug_tuple("Libsql").field(conn).finish(),
            #[cfg(feature = "turso")]
            Self::Turso { .. } => f.debug_tuple("Turso").field(&"<Connection>").finish(),
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
        translate_placeholders: bool,
    ) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
        match pool {
            #[cfg(feature = "postgres")]
            MiddlewarePool::Postgres(pool) => {
                let conn: PostgresObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorPostgres)?;
                Ok(MiddlewarePoolConnection::Postgres {
                    client: conn,
                    translate_placeholders,
                })
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePool::Sqlite(pool) => {
                let conn: SqliteObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorSqlite)?;
                let worker_conn = SqliteConnection::new(conn)?;
                Ok(MiddlewarePoolConnection::Sqlite {
                    conn: worker_conn,
                    translate_placeholders,
                })
            }
            #[cfg(feature = "mssql")]
            MiddlewarePool::Mssql(pool) => {
                let conn = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorMssql)?;
                Ok(MiddlewarePoolConnection::Mssql {
                    conn,
                    translate_placeholders,
                })
            }
            #[cfg(feature = "libsql")]
            MiddlewarePool::Libsql(pool) => {
                let conn: LibsqlObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorLibsql)?;
                Ok(MiddlewarePoolConnection::Libsql {
                    conn,
                    translate_placeholders,
                })
            }
            #[cfg(feature = "turso")]
            MiddlewarePool::Turso(db) => {
                let conn: TursoConnection = db.connect().map_err(SqlMiddlewareDbError::from)?;
                Ok(MiddlewarePoolConnection::Turso {
                    conn,
                    translate_placeholders,
                })
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }
}

impl MiddlewarePoolConnection {
    /// Run synchronous `SQLite` work on the underlying worker-owned connection.
    ///
    /// Use this when you need to batch multiple statements in one worker hop, reuse `rusqlite`
    /// features we don't expose (savepoints, pragmas that return rows, custom hooks), or avoid
    /// re-preparing statements in hot loops. It keeps blocking work off the async runtime while
    /// letting you drive the raw `rusqlite::Connection`.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError::Unimplemented`] when the connection is not `SQLite`.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.with_blocking_sqlite(|raw| {
    ///     raw.execute_batch("CREATE TABLE t (id INTEGER, name TEXT);")?;
    ///     Ok::<_, SqlMiddlewareDbError>(())
    /// })
    /// .await?;
    /// # Ok(()) }
    /// ```
    #[cfg(feature = "sqlite")]
    pub async fn with_blocking_sqlite<F, R>(&mut self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        match self {
            MiddlewarePoolConnection::Sqlite { conn, .. } => conn.with_connection(func).await,
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "with_blocking_sqlite is only available for SQLite connections".to_string(),
            )),
        }
    }

    /// Prepare a `SQLite` statement and obtain a reusable handle backed by the worker thread.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError::Unimplemented`] when the underlying connection is not
    /// `SQLite`, or propagates any preparation error reported by the worker thread.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.execute_batch("CREATE TABLE t (id INTEGER, name TEXT)").await?;
    ///
    /// let prepared = conn
    ///     .prepare_sqlite_statement("INSERT INTO t (id, name) VALUES (?1, ?2)")
    ///     .await?;
    /// prepared
    ///     .execute(&[RowValues::Int(1), RowValues::Text("alice".into())])
    ///     .await?;
    /// # Ok(()) }
    /// ```
    #[cfg(feature = "sqlite")]
    pub async fn prepare_sqlite_statement(
        &mut self,
        query: &str,
    ) -> Result<SqlitePreparedStatement, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Sqlite { conn, .. } => conn.prepare_statement(query).await,
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "prepare_sqlite_statement is only available for SQLite connections".to_string(),
            )),
        }
    }

    /// Prepare a Turso statement and obtain a reusable handle tied to the pooled connection.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError::Unimplemented`] when the connection is not Turso-enabled,
    /// or bubbles up any error returned while preparing the statement through Turso's client.
    #[cfg(feature = "turso")]
    pub async fn prepare_turso_statement(
        &mut self,
        query: &str,
    ) -> Result<TursoNonTxPreparedStatement, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => TursoNonTxPreparedStatement::prepare(turso_conn.clone(), query).await,
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "prepare_turso_statement is only available for Turso connections".to_string(),
            )),
        }
    }

    /// Pool-default translation toggle attached to this connection.
    #[must_use]
    pub fn translation_default(&self) -> bool {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres {
                translate_placeholders,
                ..
            } => *translate_placeholders,
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite {
                translate_placeholders,
                ..
            } => *translate_placeholders,
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql {
                translate_placeholders,
                ..
            } => *translate_placeholders,
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql {
                translate_placeholders,
                ..
            } => *translate_placeholders,
            #[cfg(feature = "turso")]
            MiddlewarePoolConnection::Turso {
                translate_placeholders,
                ..
            } => *translate_placeholders,
        }
    }
}
