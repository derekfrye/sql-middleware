use std::error::Error;
use std::fmt;
// use tokio::task::spawn_blocking;
use chrono::NaiveDateTime;
use deadpool_postgres::{Object as PostgresObject, Pool as DeadpoolPostgresPool};
use deadpool_sqlite::{Object as SqliteObject, Pool as DeadpoolSqlitePool};
use serde_json::Value as JsonValue;
// use deadpool_postgres::Config as PgConfig;

// use crate::db_model::ReadOnlyQuery;
// use crate::db_model::{ConfigAndPool, DatabaseType, DbError, RowValues};

// use crate::sqlite::exec_write_query_sync;
pub type SqliteWritePool = DeadpoolSqlitePool;

#[derive(Debug, Clone)]
pub struct QueryAndParams {
    pub query: String,
    pub params: Vec<RowValues>,
    pub is_read_only: bool, // Indicates if the query is read-only (SELECT)
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug)]
pub enum DbError {
    PostgresError(tokio_postgres::Error),
    SqliteError(rusqlite::Error),
    PoolErrorPostgres(deadpool::managed::PoolError<tokio_postgres::Error>),
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

impl Error for DbError {}

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

// ----------------------------------------
// Impl for ConfigAndPool
// ----------------------------------------
// impl ConfigAndPool {
//     /// One entry point. Decides at runtime if this is Postgres or SQLite.
//     pub async fn new_auto(config: &PgConfig, db_type: DatabaseType) -> Result<Self, DbError> {
//         match db_type {
//             DatabaseType::Postgres => {
//                 // We need to clone, because `TryFrom<PgConfig>` consumes the input
//                 let config_clone = config.clone();
//                 config_clone.try_into()
//             }
//             DatabaseType::Sqlite => {
//                 // For SQLite, interpret `config.dbname` as the file path
//                 let path = config
//                     .dbname
//                     .clone()
//                     .ok_or_else(|| DbError::Other("No dbname provided for SQLite".into()))?;

//                 // Initialize the ConfigAndPool using deadpool_sqlite
//                 ConfigAndPool::new_sqlite(path).await
//             }
//         }
//     }
// }
