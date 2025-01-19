// postgres.rs

use tokio_postgres::NoTls;
use deadpool_postgres::{Config as PgConfig, Runtime};


use crate::db_model::{
    ConfigAndPool, DatabaseType, DbError, MiddlewarePool, RowValues
};

// If you prefer to keep the `From<tokio_postgres::Error>` for DbError here,
// you can do so. But note we’ve already declared the variant in db_model.
impl From<tokio_postgres::Error> for DbError {
    fn from(err: tokio_postgres::Error) -> Self {
        DbError::PostgresError(err)
    }
}


impl TryFrom<PgConfig> for ConfigAndPool {
    type Error = DbError;
    fn try_from(pg_config: PgConfig) -> Result<Self, Self::Error> {
        // You might do the same logic, but panic on errors, or return a default, etc.
        // For demonstration, let's do a simpler version:

        let pg_pool = pg_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("failed to create PG pool");

            Ok(ConfigAndPool {
                db_type: DatabaseType::Postgres,
                pool: MiddlewarePool::Postgres(pg_pool),
            })
    }
}


/// Convert a single column from a Postgres row into a RowValues.
/// Note: if you need additional type mappings, add them here.
pub fn extract_pg_value(row: &tokio_postgres::Row, col_name: &str, type_name: &str) -> RowValues {
    match type_name {
        "INT4" | "INT8" | "BIGINT" | "INTEGER" | "INT" => {
            let v: i64 = row.get(col_name);
            RowValues::Int(v)
        }
        "TEXT" | "VARCHAR" => {
            let s: String = row.get(col_name);
            RowValues::Text(s)
        }
        "BOOL" | "BOOLEAN" => {
            let b: bool = row.get(col_name);
            RowValues::Bool(b)
        }
        "TIMESTAMP" | "TIMESTAMPTZ" => {
            let ts: chrono::NaiveDateTime = row.get(col_name);
            RowValues::Timestamp(ts)
        }
        "FLOAT8" | "DOUBLE PRECISION" => {
            let f: f64 = row.get(col_name);
            RowValues::Float(f)
        }
        "BYTEA" => {
            let b: Vec<u8> = row.get(col_name);
            RowValues::Blob(b)
        }
        _ => {
            // fallback
            RowValues::Null
        }
    }
}

