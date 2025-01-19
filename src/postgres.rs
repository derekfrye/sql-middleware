// postgres.rs

use deadpool_postgres::Config as PgConfig;
use tokio_postgres::NoTls;

use crate::db_model::{ConfigAndPool, DatabaseType, DbError, MiddlewarePool, RowValues};

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
        if pg_config.dbname.is_none() {
            panic!("dbname is required");
        }

        if pg_config.host.is_none() {
            panic!("host is required");
        }
        if pg_config.port.is_none() {
            panic!("port is required");
        }
        if pg_config.user.is_none() {
            panic!("user is required");
        }
        if pg_config.password.is_none() {
            panic!("password is required");
        }
        // let connection_string =
        //         format!(
        //             "host={} port={} user={} password={} dbname={}",
        //             pg_config.host.as_ref().unwrap(),
        //             pg_config.port.as_ref().unwrap(),
        //             pg_config.user.as_ref().unwrap(),
        //             pg_config.password.as_ref().unwrap(),
        //             pg_config.dbname.as_ref().unwrap()
        //         );

        let pg_config_res = pg_config.create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls);
        match pg_config_res {
            Ok(pg_pool) => Ok(ConfigAndPool {
                pool: MiddlewarePool::Postgres(pg_pool),
                db_type: DatabaseType::Postgres,
            }),
            Err(e) => {
                panic!("Failed to create deadpool_postgres pool: {}", e);
            }
        }

        // Ok(ConfigAndPool {
        //     db_type: DatabaseType::Postgres,
        //     pool: MiddlewarePool::Postgres(pg_pool),
        // })
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
