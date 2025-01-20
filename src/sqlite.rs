use deadpool_sqlite::{Config as DeadpoolSqliteConfig, Pool as DeadpoolSqlitePool, Runtime};
use rusqlite::{types::ToSqlOutput, Connection, ToSql};
use rusqlite::{OpenFlags, Transaction};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use tokio::task;

use crate::db_model::{
    ConfigAndPool, CustomDbRow, DatabaseType, DbError, MiddlewarePool, RowValues,
};

// impl TryFrom<String> for ConfigAndPool {
//     type Error = DbError;
//     fn try_from(db_path: String) -> Result<Self, Self::Error> {
//         let path_str = db_path;
//         // Build the read-write manager
//         let manager = SqliteConnectionManager::file(&path_str).with_flags(
//             OpenFlags::SQLITE_OPEN_READ_WRITE
//                 | OpenFlags::SQLITE_OPEN_CREATE
//                 | OpenFlags::SQLITE_OPEN_URI,
//         );
//         let write_pool_res = r2d2::Pool::builder().max_size(10).build(manager);
//         let write_pool = match write_pool_res {
//             Ok(p) => p,
//             Err(e) => {
//                 panic!("Failed building r2d2 pool for SQLite: {}", e);
//             }
//         };

//         // Quick test
//         if let Ok(conn) = write_pool.get() {
//             let _ = conn.execute("SELECT 1;", []);
//         }

//         // Build read-only worker
//         let ro_worker = ReadOnlyWorker::new(path_str.to_string());

//         Ok(ConfigAndPool {
//             db_type: DatabaseType::Sqlite,
//             pool: MiddlewarePool::Sqlite {
//                 read_only_worker: Arc::new(ro_worker),
//                 write_pool: Arc::new(write_pool),
//             },
//         })
//     }
// }

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Sqlite using deadpool_sqlite
    pub async fn new_sqlite(db_path: String) -> Result<Self, DbError> {
        // Configure deadpool_sqlite
        let mut cfg: DeadpoolSqliteConfig = DeadpoolSqliteConfig::new(db_path.clone());
        // cfg.pool.unwrap().max_size = 15; // Adjust based on concurrency needs
        // cfg.url = Some(db_path.clone());
        // cfg.max_size = 15; // Adjust based on concurrency needs
        // cfg.timeout = Some(std::time::Duration::from_secs(5));
        // cfg.create_if_missing = Some(true);
        // cfg.flags = Some(
        //     OpenFlags::SQLITE_OPEN_READ_WRITE
        //         | OpenFlags::SQLITE_OPEN_CREATE
        //         | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        // );

        // Create the pool
        let pool = cfg
            .create_pool(Runtime::Tokio1)
            .map_err(|e| DbError::Other(format!("Failed to create Deadpool SQLite pool: {}", e)))?;

        // Initialize the database (e.g., create tables)
        {
            let conn = pool.get().await.map_err(DbError::PoolError)?;
            conn.interact(|conn| {
                conn.execute_batch(
                    "
                    PRAGMA journal_mode = WAL;
                ",
                )
                .map_err(|e| DbError::SqliteError(e))
            })
            .await?;
        }

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Sqlite(pool),
            db_type: DatabaseType::Sqlite,
        })
    }
}

// Convert rusqlite::Error into DbError.
impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        DbError::SqliteError(err)
    }
}

// Convert r2d2::Error into DbError (since we only use r2d2 for SQLite here).
// impl From<r2d2::Error> for DbError {
//     fn from(err: r2d2::Error) -> Self {
//         DbError::Other(format!("R2D2 Pool Error: {}", err))
//     }
// }

impl From<deadpool_sqlite::InteractError> for DbError {
    fn from(err: deadpool_sqlite::InteractError) -> Self {
        DbError::Other(format!("Deadpool SQLite Interact Error: {}", err))
    }
}

// /// Executes a write-query synchronously using a write-connection.
// pub fn exec_write_query_sync(
//     conn: &Connection,
//     query: &str,
//     params: &[RowValues],
// ) -> Result<usize, DbError> {
//     let bound_params_res = convert_params(params);
//     let bound_params = match bound_params_res {
//         Ok(bp) => bp,
//         Err(e) => {
//             return Err(e);
//         }
//     };

//     let stmt_res = conn.prepare(query);
//     let mut stmt = match stmt_res {
//         Ok(s) => s,
//         Err(e) => {
//             return Err(DbError::SqliteError(e));
//         }
//     };

//     let exec_res = stmt.execute(rusqlite::params_from_iter(bound_params));
//     match exec_res {
//         Ok(rows_affected) => Ok(rows_affected),
//         Err(e) => Err(DbError::SqliteError(e)),
//     }
// }

/// A small helper for re-binding params specifically in the write context.
pub fn convert_params(params: &[RowValues]) -> Result<Vec<rusqlite::types::Value>, DbError> {
    let mut vec_values = Vec::with_capacity(params.len());
    for p in params {
        let v = match p {
            RowValues::Int(i) => rusqlite::types::Value::Integer(*i),
            RowValues::Float(f) => rusqlite::types::Value::Real(*f),
            RowValues::Text(s) => rusqlite::types::Value::Text(s.clone()),
            RowValues::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
            RowValues::Timestamp(dt) => {
                let to_sql_res = dt.to_sql();
                match to_sql_res {
                    Ok(ToSqlOutput::Owned(v)) => v,
                    Ok(ToSqlOutput::Borrowed(vref)) => vref.into(),
                    Ok(_) => rusqlite::types::Value::Null, // Handle any other Ok variants
                    Err(e) => return Err(DbError::SqliteError(e)),
                }
            }
            RowValues::Null => rusqlite::types::Value::Null,
            RowValues::JSON(jsval) => rusqlite::types::Value::Text(jsval.to_string()),
            RowValues::Blob(bytes) => rusqlite::types::Value::Blob(bytes.clone()),
        };
        vec_values.push(v);
    }
    Ok(vec_values)
}
