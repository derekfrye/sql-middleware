// db_model.rs

use chrono::NaiveDateTime;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use tokio::sync::oneshot;
use deadpool_postgres::Pool as DeadpoolPostgresPool;
use r2d2_sqlite::SqliteConnectionManager;
use r2d2::Pool as R2D2Pool;

// ==============================================
// 1) Type aliases / fundamental definitions
// ==============================================

pub type SqliteWritePool = R2D2Pool<SqliteConnectionManager>;

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
    Sqlite {
        read_only_worker: Arc<ReadOnlyWorker>,
        write_pool: Arc<SqliteWritePool>,
    },
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

#[derive(Debug, )]
pub enum DbError {
    PostgresError(tokio_postgres::Error),
    SqliteError(rusqlite::Error),
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

pub struct ReadOnlyQuery {
    pub query: String,
    pub params: Vec<RowValues>,
    pub response: oneshot::Sender<Result<ResultSet, DbError>>,
}

#[derive(Debug, Clone)]
pub struct ReadOnlyWorker {
    pub sender: Sender<ReadOnlyQuery>,
}
