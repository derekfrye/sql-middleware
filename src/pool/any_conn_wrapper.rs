//! Backend-specific raw connection wrappers for `interact_*` helpers.

#[cfg(feature = "mssql")]
use tiberius::Client as TiberiusClient;
#[cfg(feature = "mssql")]
use tokio::net::TcpStream;
#[cfg(feature = "mssql")]
use tokio_util::compat::Compat;

#[cfg(feature = "sqlite")]
use rusqlite::Connection as SqliteConnectionType;

/// Wrapper around a database connection for generic code.
///
/// This enum allows code to handle `PostgreSQL`, `SQLite`, or SQL Server
/// connections in a generic way. It is primarily used by the `interact_*` helpers
/// to hand raw driver connections to user closures without exposing pool internals.
pub enum AnyConnWrapper<'a> {
    /// `PostgreSQL` client connection
    #[cfg(feature = "postgres")]
    Postgres(&'a mut tokio_postgres::Client),
    /// `SQLite` database connection
    #[cfg(feature = "sqlite")]
    Sqlite(&'a mut SqliteConnectionType),
    /// SQL Server client connection
    #[cfg(feature = "mssql")]
    Mssql(&'a mut TiberiusClient<Compat<TcpStream>>),
    /// Turso database connection
    #[cfg(feature = "turso")]
    Turso(&'a turso::Connection),
}
