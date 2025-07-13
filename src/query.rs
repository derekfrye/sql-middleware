#[cfg(feature = "mssql")]
use tokio::net::TcpStream;
#[cfg(feature = "mssql")]
use tokio_util::compat::Compat;
#[cfg(feature = "mssql")]
use tiberius::Client as TiberiusClient;

#[cfg(feature = "sqlite")]
use deadpool_sqlite::rusqlite::Connection as SqliteConnectionType;

use crate::types::RowValues;

/// Wrapper around a database connection for generic code
/// 
/// This enum allows code to handle PostgreSQL, SQLite, or SQL Server
/// connections in a generic way.
pub enum AnyConnWrapper<'a> {
    /// PostgreSQL client connection
    #[cfg(feature = "postgres")]
    Postgres(&'a mut tokio_postgres::Client),
    /// SQLite database connection
    #[cfg(feature = "sqlite")]
    Sqlite(&'a mut SqliteConnectionType),
    /// SQL Server client connection
    #[cfg(feature = "mssql")]
    Mssql(&'a mut TiberiusClient<Compat<TcpStream>>),
}

/// A query and its parameters bundled together
/// 
/// This type makes it easier to pass around a SQL query and its
/// parameters as a single unit.
#[derive(Debug, Clone)]
pub struct QueryAndParams {
    /// The SQL query string
    pub query: String,
    /// The parameters to be bound to the query
    pub params: Vec<RowValues>,
}

impl QueryAndParams {
    /// Create a new QueryAndParams with the given query string and parameters
    ///
    /// # Arguments
    ///
    /// * `query` - The SQL query string
    /// * `params` - The parameters to bind to the query
    ///
    /// # Returns
    ///
    /// A new QueryAndParams instance
    pub fn new(query: impl Into<String>, params: Vec<RowValues>) -> Self {
        Self {
            query: query.into(),
            params,
        }
    }
    
    /// Create a new QueryAndParams with no parameters
    ///
    /// # Arguments
    ///
    /// * `query` - The SQL query string
    ///
    /// # Returns
    ///
    /// A new QueryAndParams instance with an empty parameter list
    pub fn new_without_params(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            params: Vec::new(),
        }
    }
}