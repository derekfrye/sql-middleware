// All thiserror imports are handled by the macro
use async_trait::async_trait;
use chrono::NaiveDateTime;
use clap::ValueEnum;
use deadpool_postgres::{Object as PostgresObject, Pool as DeadpoolPostgresPool};
use deadpool_sqlite::{rusqlite, Object as SqliteObject, Pool as DeadpoolSqlitePool};
use rusqlite::Connection as SqliteConnectionType;
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::{postgres, sqlite};
pub type SqliteWritePool = DeadpoolSqlitePool;

pub enum AnyConnWrapper<'a> {
    Postgres(&'a mut tokio_postgres::Client),
    Sqlite(&'a mut SqliteConnectionType),
}

#[derive(Debug, Clone)]
pub struct QueryAndParams {
    pub query: String,
    pub params: Vec<RowValues>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowValues {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    Timestamp(NaiveDateTime),
    Null,
    JSON(JsonValue),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum MiddlewarePool {
    Postgres(DeadpoolPostgresPool),
    Sqlite(DeadpoolSqlitePool),
}

#[derive(Debug, Clone, PartialEq, ValueEnum)]
pub enum DatabaseType {
    Postgres,
    Sqlite,
}

#[derive(Clone, Debug)]
pub struct ConfigAndPool {
    pub pool: MiddlewarePool,
    pub db_type: DatabaseType,
}

#[derive(Debug, Error)]
pub enum SqlMiddlewareDbError {
    #[error(transparent)]
    PostgresError(#[from] tokio_postgres::Error),
    
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    
    #[error(transparent)]
    PoolErrorPostgres(#[from] deadpool::managed::PoolError<tokio_postgres::Error>),
    
    #[error(transparent)]
    PoolErrorSqlite(#[from] deadpool::managed::PoolError<rusqlite::Error>),
    
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

#[derive(Debug, Clone)]
pub struct CustomDbRow {
    pub column_names: std::sync::Arc<Vec<String>>,
    pub rows: Vec<RowValues>,
}

impl CustomDbRow {
    pub fn get_column_index(&self, column_name: &str) -> Option<usize> {
        self.column_names.iter().position(|col| col == column_name)
    }

    pub fn get(&self, column_name: &str) -> Option<&RowValues> {
        let index_opt = self.get_column_index(column_name);
        if let Some(idx) = index_opt {
            self.rows.get(idx)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResultSet {
    pub results: Vec<CustomDbRow>,
    pub rows_affected: usize,
}

impl ResultSet {
    pub fn new() -> ResultSet {
        ResultSet {
            results: vec![],
            rows_affected: 0,
        }
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
            MiddlewarePool::Postgres(pool) => {
                let conn: PostgresObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorPostgres)?;
                Ok(MiddlewarePoolConnection::Postgres(conn))
            }
            MiddlewarePool::Sqlite(pool) => {
                let conn: SqliteObject = pool
                    .get()
                    .await
                    .map_err(SqlMiddlewareDbError::PoolErrorSqlite)?;
                Ok(MiddlewarePoolConnection::Sqlite(conn))
            }
        }
    }
}

#[derive(Debug)]
pub enum MiddlewarePoolConnection {
    Postgres(PostgresObject),
    Sqlite(SqliteObject),
}

impl MiddlewarePoolConnection {
    pub async fn interact_async<F, Fut>(
        &mut self,
        func: F,
    ) -> Result<Fut::Output, SqlMiddlewareDbError>
    where
        F: FnOnce(AnyConnWrapper<'_>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), SqlMiddlewareDbError>> + Send + 'static,
    {
        match self {
            MiddlewarePoolConnection::Postgres(pg_obj) => {
                // Assuming PostgresObject dereferences to tokio_postgres::Client
                let client: &mut tokio_postgres::Client = pg_obj.as_mut();
                Ok(func(AnyConnWrapper::Postgres(client)).await)
            }
            MiddlewarePoolConnection::Sqlite(_) => {
                Err(SqlMiddlewareDbError::Unimplemented(
                    "interact_async is not supported for SQLite; use interact_sync instead".to_string()
                ))
            }
        }
    }

    pub async fn interact_sync<F, R>(&self, f: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(AnyConnWrapper) -> R + Send + 'static,
        R: Send + 'static,
    {
        match self {
            MiddlewarePoolConnection::Sqlite(sqlite_obj) => {
                // Use `deadpool_sqlite`'s `interact` method
                sqlite_obj
                    .interact(move |conn| {
                        let wrapper = AnyConnWrapper::Sqlite(conn);
                        Ok(f(wrapper))
                    })
                    .await?
            }
            MiddlewarePoolConnection::Postgres(_) => {
                Err(SqlMiddlewareDbError::Unimplemented(
                    "interact_sync is not supported for Postgres; use interact_async instead".to_string()
                ))
            }
        }
    }
}

// ----------------------------------------
// Common impl blocks for DbError
// ----------------------------------------
// We don't need this anymore as thiserror already generates a Display implementation
// The #[error] attributes on the enum variants define the format for each variant
// This is commented out to show what was here originally
/*
impl fmt::Display for SqlMiddlewareDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqlMiddlewareDbError::PostgresError(e) => write!(f, "PostgresError: {}", e),
            SqlMiddlewareDbError::SqliteError(e) => write!(f, "SqliteError: {}", e),
            SqlMiddlewareDbError::ConfigError(msg) => write!(f, "ConfigError: {}", msg),
            SqlMiddlewareDbError::ConnectionError(msg) => write!(f, "ConnectionError: {}", msg),
            SqlMiddlewareDbError::ParameterError(msg) => write!(f, "ParameterError: {}", msg),
            SqlMiddlewareDbError::ExecutionError(msg) => write!(f, "ExecutionError: {}", msg),
            SqlMiddlewareDbError::Unimplemented(msg) => write!(f, "Unimplemented: {}", msg),
            SqlMiddlewareDbError::Other(msg) => write!(f, "Other: {}", msg),
            SqlMiddlewareDbError::PoolErrorSqlite(e) => write!(f, "PoolError: {:?}", e),
            SqlMiddlewareDbError::PoolErrorPostgres(pool_error) => {
                write!(f, "PoolErrorPostgres: {:?}", pool_error)
            }
        }
    }
}
*/

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
            return Some(value.clone());
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

    pub fn as_json(&self) -> Option<&serde_json::Value> {
        if let RowValues::JSON(value) = self {
            Some(value)
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

    pub fn as_float(&self) -> Option<f64> {
        if let RowValues::Float(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, RowValues::Null)
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
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_batch(pg_client, query).await
            }
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_batch(sqlite_client, query).await
            }
        }
    }
    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_select(pg_client, query, params).await
            }
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_select(sqlite_client, query, params).await
            }
        }
    }
    async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_dml(pg_client, query, &params).await
            }
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_dml(sqlite_client, query, params).await
            }
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
