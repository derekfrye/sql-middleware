#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;
#[cfg(feature = "postgres")]
use bb8::Pool as PostgresPool;

#[cfg(feature = "sqlite")]
use crate::sqlite::config::SqliteManager;
#[cfg(feature = "sqlite")]
use bb8::Pool as Bb8SqlitePool;

#[cfg(feature = "mssql")]
use bb8::Pool as Bb8MssqlPool;
#[cfg(feature = "mssql")]
use bb8_tiberius::ConnectionManager;

#[cfg(feature = "turso")]
use turso::Database as TursoDatabase;

use crate::error::SqlMiddlewareDbError;

/// Connection pool for database access
///
/// This enum wraps the different connection pool types for the
/// supported database engines.
#[derive(Clone)]
pub enum MiddlewarePool {
    /// `PostgreSQL` connection pool
    #[cfg(feature = "postgres")]
    Postgres(PostgresPool<PgManager>),
    /// `SQLite` connection pool
    #[cfg(feature = "sqlite")]
    Sqlite(Bb8SqlitePool<SqliteManager>),
    /// SQL Server connection pool
    #[cfg(feature = "mssql")]
    Mssql(Bb8MssqlPool<ConnectionManager>),
    /// `Turso` pseudo-pool (Database handle)
    #[cfg(feature = "turso")]
    Turso(TursoDatabase),
}

// Manual Debug implementation because not all pool types expose `Debug`
impl std::fmt::Debug for MiddlewarePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(pool) => f.debug_tuple("Postgres").field(pool).finish(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(pool) => f.debug_tuple("Sqlite").field(pool).finish(),
            #[cfg(feature = "mssql")]
            Self::Mssql(_) => f.debug_tuple("Mssql").field(&"<TiberiusPool>").finish(),
            #[cfg(feature = "turso")]
            Self::Turso(_) => f.debug_tuple("Turso").field(&"<Database>").finish(),
        }
    }
}

impl MiddlewarePool {
    /// Return a reference to self instead of cloning the entire pool
    ///
    /// # Errors
    /// This function currently never returns an error but maintains Result for API consistency.
    #[allow(clippy::unused_async)]
    pub async fn get(&self) -> Result<&MiddlewarePool, SqlMiddlewareDbError> {
        Ok(self)
    }
}
