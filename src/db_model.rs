// db_model.rs

use chrono::NaiveDateTime;
use deadpool::managed::{Manager, Object};
use deadpool_postgres::{
    Manager as PostgresManager, Object as PostgresObject, Pool as DeadpoolPostgresPool,
};
use deadpool_sqlite::{
    Manager as SqliteManager, Object as SqliteObject, Pool as DeadpoolSqlitePool,
};
use serde_json::Value as JsonValue;
// use std::sync::mpsc::Sender;
use std::sync::Arc;
// use tokio::sync::oneshot;

// ==============================================
// 1) Type aliases / fundamental definitions
// ==============================================

pub type SqliteWritePool = DeadpoolSqlitePool;

// ==============================================
// 2) Structs / Enums WITHOUT impl blocks
// ==============================================

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
