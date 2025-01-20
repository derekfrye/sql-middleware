use std::error::Error;
use std::fmt;
// use tokio::task::spawn_blocking;

use deadpool_postgres::Config as PgConfig;

// use crate::db_model::ReadOnlyQuery;
use crate::db_model::{ConfigAndPool, DatabaseType, DbError, RowValues};

// use crate::sqlite::exec_write_query_sync;

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
impl ConfigAndPool {
    /// One entry point. Decides at runtime if this is Postgres or SQLite.
    pub async fn new_auto(config: &PgConfig, db_type: DatabaseType) -> Result<Self, DbError> {
        match db_type {
            DatabaseType::Postgres => {
                // We need to clone, because `TryFrom<PgConfig>` consumes the input
                let config_clone = config.clone();
                config_clone.try_into()
            }
            DatabaseType::Sqlite => {
                // For SQLite, interpret `config.dbname` as the file path
                let path = config
                    .dbname
                    .clone()
                    .ok_or_else(|| DbError::Other("No dbname provided for SQLite".into()))?;

                // Initialize the ConfigAndPool using deadpool_sqlite
                ConfigAndPool::new_sqlite(path).await
            }
        }
    }
}
