// All thiserror imports are handled by the macro
use async_trait::async_trait;
use chrono::NaiveDateTime;
use clap::ValueEnum;
use serde_json::Value as JsonValue;
use thiserror::Error;

#[cfg(feature = "mssql")]
use std::ops::DerefMut;

// Conditional imports based on enabled features
#[cfg(feature = "postgres")]
use deadpool_postgres::{Object as PostgresObject, Pool as DeadpoolPostgresPool};
#[cfg(feature = "postgres")]
use tokio_postgres;

#[cfg(feature = "sqlite")]
use deadpool_sqlite::{rusqlite, Object as SqliteObject, Pool as DeadpoolSqlitePool};
#[cfg(feature = "sqlite")]
use rusqlite::Connection as SqliteConnectionType;
#[cfg(feature = "sqlite")]
pub type SqliteWritePool = DeadpoolSqlitePool;

#[cfg(feature = "mssql")]
use tokio::net::TcpStream;
#[cfg(feature = "mssql")]
use tokio_util::compat::Compat;
#[cfg(feature = "mssql")]
use tiberius::Client as TiberiusClient;
#[cfg(feature = "mssql")]
use deadpool_tiberius::Pool as TiberiusPool;

// Conditional module imports
#[cfg(feature = "postgres")]
use crate::postgres;
#[cfg(feature = "sqlite")]
use crate::sqlite;
#[cfg(feature = "mssql")]
use crate::mssql;

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

/// Values that can be stored in a database row or used as query parameters
///
/// This enum provides a unified representation of database values across
/// different database engines.
#[derive(Debug, Clone, PartialEq)]
pub enum RowValues {
    /// Integer value (64-bit)
    Int(i64),
    /// Floating point value (64-bit)
    Float(f64),
    /// Text/string value
    Text(String),
    /// Boolean value
    Bool(bool),
    /// Timestamp value
    Timestamp(NaiveDateTime),
    /// NULL value
    Null,
    /// JSON value
    JSON(JsonValue),
    /// Binary data
    Blob(Vec<u8>),
}

impl RowValues {
    /// Check if this value is NULL
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// The database type supported by this middleware
#[derive(Debug, Clone, PartialEq, Eq, Hash, ValueEnum)]
pub enum DatabaseType {
    /// PostgreSQL database
    #[cfg(feature = "postgres")]
    Postgres,
    /// SQLite database
    #[cfg(feature = "sqlite")]
    Sqlite,
    /// SQL Server database
    #[cfg(feature = "mssql")]
    Mssql,
}

/// Connection pool for database access
///
/// This enum wraps the different connection pool types for the
/// supported database engines.
#[derive(Clone)]
pub enum MiddlewarePool {
    /// PostgreSQL connection pool
    #[cfg(feature = "postgres")]
    Postgres(DeadpoolPostgresPool),
    /// SQLite connection pool
    #[cfg(feature = "sqlite")]
    Sqlite(DeadpoolSqlitePool),
    /// SQL Server connection pool
    #[cfg(feature = "mssql")]
    Mssql(TiberiusPool),
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

/// A row from a database query result
///
/// This struct represents a single row from a database query result,
/// with access to both the column names and the values.
#[derive(Debug, Clone)]
pub struct CustomDbRow {
    /// The column names for this row (shared across all rows in a result set)
    pub column_names: std::sync::Arc<Vec<String>>,
    /// The values for this row
    pub rows: Vec<RowValues>,
    // Internal cache for faster column lookups (to avoid repeated string comparisons)
    #[doc(hidden)]
    column_index_cache: std::sync::Arc<std::collections::HashMap<String, usize>>,
}

impl CustomDbRow {
    /// Create a new database row
    ///
    /// # Arguments
    ///
    /// * `column_names` - The column names
    /// * `rows` - The values for this row
    ///
    /// # Returns
    ///
    /// A new CustomDbRow instance
    pub fn new(column_names: std::sync::Arc<Vec<String>>, rows: Vec<RowValues>) -> Self {
        // Build a cache of column name to index for faster lookups
        let cache = std::sync::Arc::new(
            column_names
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), i))
                .collect::<std::collections::HashMap<_, _>>()
        );
        
        Self { 
            column_names, 
            rows,
            column_index_cache: cache,
        }
    }
    
    /// Get the index of a column by name
    ///
    /// # Arguments
    ///
    /// * `column_name` - The name of the column
    ///
    /// # Returns
    ///
    /// The index of the column, or None if not found
    pub fn get_column_index(&self, column_name: &str) -> Option<usize> {
        // First check the cache
        if let Some(&idx) = self.column_index_cache.get(column_name) {
            return Some(idx);
        }
        
        // Fall back to linear search
        self.column_names.iter().position(|col| col == column_name)
    }

    /// Get a value from the row by column name
    ///
    /// # Arguments
    ///
    /// * `column_name` - The name of the column
    ///
    /// # Returns
    ///
    /// The value at the column, or None if the column wasn't found
    pub fn get(&self, column_name: &str) -> Option<&RowValues> {
        let index_opt = self.get_column_index(column_name);
        if let Some(idx) = index_opt {
            self.rows.get(idx)
        } else {
            None
        }
    }
    
    /// Get a value from the row by column index
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the column
    ///
    /// # Returns
    ///
    /// The value at the index, or None if the index is out of bounds
    pub fn get_by_index(&self, index: usize) -> Option<&RowValues> {
        self.rows.get(index)
    }
}

/// A result set from a database query
///
/// This struct represents the result of a database query,
/// containing the rows returned by the query and metadata.
#[derive(Debug, Clone, Default)]
pub struct ResultSet {
    /// The rows returned by the query
    pub results: Vec<CustomDbRow>,
    /// The number of rows affected (for DML statements)
    pub rows_affected: usize,
    /// Column names shared by all rows (to avoid duplicating in each row)
    column_names: Option<std::sync::Arc<Vec<String>>>,
}

impl ResultSet {
    /// Create a new result set with a known capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - The initial capacity for the result rows
    ///
    /// # Returns
    ///
    /// A new ResultSet instance with preallocated capacity
    pub fn with_capacity(capacity: usize) -> ResultSet {
        ResultSet {
            results: Vec::with_capacity(capacity),
            rows_affected: 0,
            column_names: None,
        }
    }
    
    /// Set the column names for this result set (to be shared by all rows)
    pub fn set_column_names(&mut self, column_names: std::sync::Arc<Vec<String>>) {
        self.column_names = Some(column_names);
    }
    
    /// Get the column names for this result set
    pub fn get_column_names(&self) -> Option<&std::sync::Arc<Vec<String>>> {
        self.column_names.as_ref()
    }
    
    /// Add a row to the result set
    ///
    /// # Arguments
    ///
    /// * `row_values` - The values for this row
    pub fn add_row_values(&mut self, row_values: Vec<RowValues>) {
        if let Some(column_names) = &self.column_names {
            // Build a cache of column name to index for faster lookups
            // We only need to build this cache once and reuse it
            lazy_static::lazy_static! {
                static ref CACHE_MAP: std::sync::Mutex<std::collections::HashMap<usize, std::sync::Arc<std::collections::HashMap<String, usize>>>> = std::sync::Mutex::new(std::collections::HashMap::new());
            }
            
            // Use the pointer to column_names as a key for the cache
            let ptr = column_names.as_ref().as_ptr() as usize;
            let cache = {
                let mut cache_map = CACHE_MAP.lock().unwrap();
                let cache_entry = cache_map.entry(ptr).or_insert_with(|| {
                    std::sync::Arc::new(
                        column_names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| (name.to_string(), i))
                            .collect::<std::collections::HashMap<_, _>>()
                    )
                });
                cache_entry.clone()
            };
            
            let row = CustomDbRow {
                column_names: column_names.clone(),
                rows: row_values,
                column_index_cache: cache,
            };
            
            self.results.push(row);
            self.rows_affected += 1;
        }
    }
    
    /// Add a row to the result set (legacy method, less efficient)
    ///
    /// # Arguments
    ///
    /// * `row` - The row to add
    pub fn add_row(&mut self, row: CustomDbRow) {
        // If column names haven't been set yet, use the ones from this row
        if self.column_names.is_none() {
            self.column_names = Some(row.column_names.clone());
        }
        
        self.results.push(row);
        self.rows_affected += 1;
    }
}

impl MiddlewarePool {
    // Return a reference to self instead of cloning the entire pool
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
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string()
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
            Self::Mssql(_) => f.debug_tuple("Mssql").field(&"<TiberiusConnection>").finish(),
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
                let client = mssql_obj.deref_mut();
                Ok(func(AnyConnWrapper::Mssql(client)).await)
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(_) => {
                Err(SqlMiddlewareDbError::Unimplemented(
                    "interact_async is not supported for SQLite; use interact_sync instead".to_string()
                ))
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_async is not implemented for this database type".to_string()
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
            MiddlewarePoolConnection::Postgres(_) => {
                Err(SqlMiddlewareDbError::Unimplemented(
                    "interact_sync is not supported for Postgres; use interact_async instead".to_string()
                ))
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(_) => {
                Err(SqlMiddlewareDbError::Unimplemented(
                    "interact_sync is not supported for SQL Server; use interact_async instead".to_string()
                ))
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not implemented for this database type".to_string()
            )),
        }
    }
}

// ----------------------------------------
// Common impl blocks for DbError
// ----------------------------------------
// The #[error] attributes on the enum variants define the format for each variant

// ----------------------------------------
// Impl for RowValues that is DB-agnostic
// ----------------------------------------
impl RowValues {
    pub fn as_int(&self) -> Option<&i64> {
        if let RowValues::Int(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        if let RowValues::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<&bool> {
        if let RowValues::Bool(value) = self {
            return Some(value);
        } else if let Some(i) = self.as_int() {
            if *i == 1 {
                return Some(&true);
            } else if *i == 0 {
                return Some(&false);
            }
        }
        None
    }

    pub fn as_timestamp(&self) -> Option<chrono::NaiveDateTime> {
        if let RowValues::Timestamp(value) = self {
            return Some(*value); // Use dereferencing instead of clone for Copy types
        } else if let Some(s) = self.as_text() {
            // Try "YYYY-MM-DD HH:MM:SS"
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
                return Some(dt);
            }
            // Try "YYYY-MM-DD HH:MM:SS.SSS"
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S.%3f") {
                return Some(dt);
            }
        }
        None
    }

    pub fn as_float(&self) -> Option<f64> {
        if let RowValues::Float(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    pub fn as_blob(&self) -> Option<&[u8]> {
        if let RowValues::Blob(bytes) = self {
            Some(bytes)
        } else {
            None
        }
    }
}

#[async_trait]
pub trait AsyncDatabaseExecutor {
    /// Executes a batch of SQL queries (can be a mix of reads/writes) within a transaction. No parameters are supported.
    async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError>;

    /// Executes a single SELECT statement and returns the result set.
    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError>;

    /// Executes a single DML statement (INSERT, UPDATE, DELETE, etc.) and returns the number of rows affected.
    async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError>;
}

#[async_trait]
impl AsyncDatabaseExecutor for MiddlewarePoolConnection {
    /// Executes a batch of SQL queries within a transaction by delegating to the specific database module.
    async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_batch(pg_client, query).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_batch(sqlite_client, query).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_client) => {
                mssql::execute_batch(mssql_client, query).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string()
            )),
        }
    }
    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_select(pg_client, query, params).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_select(sqlite_client, query, params).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_client) => {
                mssql::execute_select(mssql_client, query, params).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string()
            )),
        }
    }
    async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_dml(pg_client, query, params).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_dml(sqlite_client, query, params).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_client) => {
                mssql::execute_dml(mssql_client, query, params).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string()
            )),
        }
    }
}

/// Convert a slice of RowValues into database-specific parameters.
/// This trait provides a unified interface for converting generic RowValues
/// to database-specific parameter types.
pub trait ParamConverter<'a> {
    type Converted;

    /// Convert a slice of RowValues into the backend’s parameter type.
    fn convert_sql_params(
        params: &'a [RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError>;
    
    /// Check if this converter supports the given mode
    /// 
    /// # Arguments
    /// * `mode` - The conversion mode to check
    /// 
    /// # Returns
    /// * `bool` - Whether this converter supports the mode
    fn supports_mode(_mode: ConversionMode) -> bool {
        true // By default, support both modes
    }
}

/// The conversion "mode".
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConversionMode {
    /// When the converted parameters will be used in a query (SELECT)
    Query,
    /// When the converted parameters will be used for statement execution (INSERT/UPDATE/etc.)
    Execute,
}
