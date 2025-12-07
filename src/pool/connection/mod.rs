mod libsql;
mod mssql;
mod postgres;
mod sqlite;
mod turso;

#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;
#[cfg(any(feature = "postgres", feature = "mssql"))]
use bb8::PooledConnection;
#[cfg(feature = "mssql")]
use bb8_tiberius::ConnectionManager;

use super::types::MiddlewarePool;
use crate::error::SqlMiddlewareDbError;
#[cfg(feature = "sqlite")]
use crate::sqlite::SqliteConnection;
#[cfg(feature = "libsql")]
use deadpool_libsql::Object as LibsqlObject;

#[cfg(feature = "turso")]
use ::turso::Connection as TursoConnection;

pub enum MiddlewarePoolConnection {
    #[cfg(feature = "postgres")]
    Postgres {
        client: PooledConnection<'static, PgManager>,
        translate_placeholders: bool,
    },
    #[cfg(feature = "sqlite")]
    Sqlite {
        conn: Option<SqliteConnection>,
        translate_placeholders: bool,
    },
    #[cfg(feature = "mssql")]
    Mssql {
        conn: PooledConnection<'static, ConnectionManager>,
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

// Manual Debug implementation because some pool variants do not expose `Debug`
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
                postgres::get_connection(pool, translate_placeholders).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePool::Sqlite(pool) => {
                sqlite::get_connection(pool, translate_placeholders).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePool::Mssql(pool) => {
                mssql::get_connection(pool, translate_placeholders).await
            }
            #[cfg(feature = "libsql")]
            MiddlewarePool::Libsql(pool) => {
                libsql::get_connection(pool, translate_placeholders).await
            }
            #[cfg(feature = "turso")]
            MiddlewarePool::Turso(db) => turso::get_connection(db, translate_placeholders),
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }
}

impl MiddlewarePoolConnection {
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
