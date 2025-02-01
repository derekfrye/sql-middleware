// use std::error::Error;
use std::{fmt, future::Future};
// use tokio::task::spawn_blocking;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use clap::ValueEnum;
use deadpool_postgres::{Object as PostgresObject, Pool as DeadpoolPostgresPool};
use deadpool_sqlite::{rusqlite, Object as SqliteObject, Pool as DeadpoolSqlitePool};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::{postgres, sqlite};
pub type SqliteWritePool = DeadpoolSqlitePool;

#[derive(Debug, Clone)]
pub struct QueryAndParams {
    pub query: String,
    pub params: Vec<RowValues>,
    pub is_read_only: bool, // Indicates if the query is read-only (SELECT)
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

impl MiddlewarePool {
    pub async fn get(&self) -> Result<MiddlewarePool, DbError> {
        match self {
            MiddlewarePool::Postgres(pool) => {
                let pool = pool.clone();
                Ok(MiddlewarePool::Postgres(pool))
            }
            MiddlewarePool::Sqlite(pool) => {
                let pool = pool.clone();
                Ok(MiddlewarePool::Sqlite(pool))
            }
        }
    }
    pub async fn get_connection(pool: MiddlewarePool) -> Result<MiddlewarePoolConnection, DbError> {
        match pool {
            MiddlewarePool::Postgres(pool) => {
                let conn: PostgresObject = pool.get().await.map_err(DbError::PoolErrorPostgres)?;
                Ok(MiddlewarePoolConnection::Postgres(conn))
            }
            MiddlewarePool::Sqlite(pool) => {
                let conn: SqliteObject = pool.get().await.map_err(DbError::PoolErrorSqlite)?;
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

#[derive(Clone, Debug)]
pub struct Db {
    pub config_and_pool: ConfigAndPool,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error(transparent)]
    PostgresError(tokio_postgres::Error),
    #[error(transparent)]
    SqliteError(rusqlite::Error),
    #[error(transparent)]
    PoolErrorPostgres(deadpool::managed::PoolError<tokio_postgres::Error>),
    #[error(transparent)]
    PoolErrorSqlite(deadpool::managed::PoolError<rusqlite::Error>),

    Other(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum QueryState {
    #[default]
    NoConnection,
    MissingRelations,
    QueryReturnedSuccessfully,
    QueryError,
}

#[derive(Debug, Clone)]
pub struct CustomDbRow {
    pub column_names: Vec<String>,
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

#[derive(Debug, Clone, Default)]
pub struct DatabaseResult<T> {
    pub return_result: T,
    pub db_last_exec_state: QueryState,
    pub error_message: Option<String>,
    pub db_object_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckType {
    Table,
    Constraint,
}

// ----------------------------------------
// Common impl blocks for DbError
// ----------------------------------------
impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::PostgresError(e) => write!(f, "PostgresError: {}", e),
            DbError::SqliteError(e) => write!(f, "SqliteError: {}", e),
            DbError::Other(msg) => write!(f, "Other: {}", msg),
            DbError::PoolErrorSqlite(e) => write!(f, "PoolError: {:?}", e),
            DbError::PoolErrorPostgres(pool_error) => {
                write!(f, "PoolErrorPostgres: {:?}", pool_error)
            }
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct DatabaseTable {
    pub table_name: String,
    pub ddl: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConstraint {
    pub table_name: String,
    pub constraint_name: String,
    pub constraint_type: String,
    pub ddl: String,
}

#[derive(Debug, Clone)]
pub enum DatabaseItem {
    Table(DatabaseTable),
    Constraint(DatabaseConstraint),
}

#[async_trait]
pub trait AsyncDatabaseExecutor {
    /// Executes a batch of SQL queries (can be a mix of reads/writes) within a transaction. No parameters are supported.
    async fn execute_batch(&mut self, query: &str) -> Result<(), DbError>;

    /// Executes a single SELECT statement and returns the result set.
    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, DbError>;

    /// Executes a single DML statement (INSERT, UPDATE, DELETE, etc.) and returns the number of rows affected.
    async fn execute_dml(&mut self, query: &str, params: &[RowValues]) -> Result<usize, DbError>;

    async fn interact_async<F, FR, R>(&self, f: F) -> Result<R, DbError>
    where
        // For example:
        // f: |AnyConnWrapper| async move { ...some DB calls...; Ok(...) }
        F: FnOnce(AnyConnWrapper) -> FR + Send + 'static,
        FR: Future<Output = Result<R, DbError>> + Send + 'static,
        R: Send + 'static;
}

#[async_trait]
pub trait SyncDatabaseExecutor {
    async fn interact_sync<F, R>(&self, f: F) -> Result<R, DbError>
    where
        F: FnOnce(AnyConnWrapper) -> R + Send + 'static,
        R: Send + 'static;
}

/// A trait representing a database transaction.
#[async_trait]
pub trait TransactionExecutor {
    /// Executes a single query within the transaction.
    async fn execute(&mut self, query: &str, params: &[RowValues]) -> Result<usize, DbError>;

    /// Executes a batch of queries within the transaction.
    async fn batch_execute(&mut self, query: &str) -> Result<(), DbError>;
}

/// A trait representing a prepared statement.
#[async_trait]
pub trait StatementExecutor {
    /// Executes the prepared statement and returns the result set.
    async fn execute_select(&mut self, params: &[RowValues]) -> Result<ResultSet, DbError>;
}

#[async_trait]
impl AsyncDatabaseExecutor for MiddlewarePoolConnection {
    /// Executes a batch of SQL queries within a transaction by delegating to the specific database module.
    async fn execute_batch(&mut self, query: &str) -> Result<(), DbError> {
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
    ) -> Result<ResultSet, DbError> {
        match self {
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_select(pg_client, query, params).await
            }
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_select(sqlite_client, query, params).await
            }
        }
    }
    async fn execute_dml(&mut self, query: &str, params: &[RowValues]) -> Result<usize, DbError> {
        match self {
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_dml(pg_client, query, &params).await
            }
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_dml(sqlite_client, query, params).await
            }
        }
    }
    async fn interact_async<F, FR, R>(&self, f: F) -> Result<R, DbError>
    where
        // For example:
        // f: |AnyConnWrapper| async move { ...some DB calls...; Ok(...) }
        F: FnOnce(AnyConnWrapper) -> FR + Send + 'static,
        FR: Future<Output = Result<R, DbError>> + Send + 'static,
        R: Send + 'static,
    {
        match self {
            MiddlewarePoolConnection::Postgres(pg_obj) => {
                // For Postgres, we can do fully async.
                // `pg_obj` is a `deadpool_postgres::Object` which derefs to `&tokio_postgres::Client`.
                // We'll clone it (Arc clone) to pass into the closure as an owned Client,
                // so the closure can hold onto it while it's async.
                // let pg_obj_clone = pg_obj.clone();
                let wrapper = AnyConnWrapper::Postgres(pg_obj);
                // Now just await the user closure directly
                f(wrapper).await
            }
            _ => Err(DbError::Other(
                "interact_async only supported for Postgres".into(),
            )),
        }
    }
}

#[async_trait]
impl SyncDatabaseExecutor for MiddlewarePoolConnection {
    async fn interact_sync<F, R>(&self, f: F) -> Result<R, DbError>
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
            _ => Err(DbError::Other(
                "interact_sync only supported for SQLite".into(),
            )),
        }
    }
}

impl MiddlewarePoolConnection {
    /// Async transaction closure.  
    /// `f` is an async closure that will receive an `AnyConnWrapper`,  
    /// and returns a `Future<Output=Result<R>>`.  
    pub async fn interact_async<F, FR, R>(&self, f: F) -> Result<R, DbError>
    where
        // For example:
        // f: |AnyConnWrapper| async move { ...some DB calls...; Ok(...) }
        F: FnOnce(AnyConnWrapper) -> FR + Send + 'static,
        FR: Future<Output = Result<R, DbError>> + Send + 'static,
        R: Send + 'static,
    {
        match self {
            MiddlewarePoolConnection::Sqlite(sqlite_obj) => {
                // For SQLite, we must call `rusqlite` operations in a blocking manner.
                // `deadpool_sqlite` has `.interact(...)` which gives us a synchronous closure.
                // Inside that closure, we "block_on" your async code.
                sqlite_obj
                    .interact(move |conn| {
                        let wrapper = AnyConnWrapper::Sqlite(conn);
                        // We have to block_on the async closure because rusqlite is sync.
                        tokio::runtime::Handle::current().block_on(f(wrapper))
                    })
                    .await?
            }
            MiddlewarePoolConnection::Postgres(pg_obj) => {
                // For Postgres, we can do fully async.
                // `pg_obj` is a `deadpool_postgres::Object` which derefs to `&tokio_postgres::Client`.
                // We'll clone it (Arc clone) to pass into the closure as an owned Client,
                // so the closure can hold onto it while it's async.
                // let pg_obj_clone = pg_obj.clone();
                let wrapper = AnyConnWrapper::Postgres(pg_obj);
                // Now just await the user closure directly
                f(wrapper).await
            }
        }
    }
}

//
// 5. AnyConnWrapper: an enum with access to the actual underlying rusqlite::Connection
//    or a (cloned) tokio_postgres::Client.
//
//    Because a `deadpool_postgres::Object` is basically an `Arc<tokio_postgres::Client>` plus
//    some manager info, we can store that in Postgres variant.
//
pub enum AnyConnWrapper<'c> {
    /// SQLite is sync. We hold a reference to the `rusqlite::Connection`.
    Sqlite(&'c rusqlite::Connection),
    /// Postgres: store the entire `deadpool_postgres::Object`, which can be treated as a
    /// clonable handle to the underlying tokio_postgres::Client.
    Postgres(&'c deadpool_postgres::Object),
}
