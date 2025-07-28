#[cfg(feature = "postgres")]
use deadpool_postgres::Pool as DeadpoolPostgresPool;

#[cfg(feature = "sqlite")]
use deadpool_sqlite::Pool as DeadpoolSqlitePool;

#[cfg(feature = "sqlite")]
pub type SqliteWritePool = DeadpoolSqlitePool;

#[cfg(feature = "mssql")]
use deadpool_tiberius::Pool as TiberiusPool;

#[cfg(feature = "libsql")]
use deadpool_libsql::Pool as DeadpoolLibsqlPool;

use crate::error::SqlMiddlewareDbError;

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