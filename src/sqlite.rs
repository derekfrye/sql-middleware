// sqlite.rs

use std::sync::mpsc::{Sender, Receiver};
use std::sync::Arc;
use std::thread;
use tokio::sync::oneshot;
use rusqlite::{Connection, types::ToSqlOutput,ToSql};
use r2d2::PooledConnection;
use rusqlite::OpenFlags;
use r2d2_sqlite::SqliteConnectionManager;

use crate::db_model::{
    ConfigAndPool, CustomDbRow, DatabaseType, DbError, MiddlewarePool, ReadOnlyQuery, ReadOnlyWorker, ResultSet, RowValues, SqliteWritePool
};

impl TryFrom<String> for ConfigAndPool {
    type Error = DbError;
    fn try_from(db_path: String) -> Result<Self, Self::Error> {
        let path_str = db_path;
        // Build the read-write manager
        let manager = SqliteConnectionManager::file(&path_str).with_flags(
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        );
        let write_pool = r2d2::Pool::builder()
            .max_size(10)
            .build(manager)
            .map_err(|e| DbError::Other(format!("r2d2 error: {}", e))).unwrap();

        // Quick test
        if let Ok(conn) = write_pool.get() {
            let _ = conn.execute("SELECT 1;", []);
        }

        // Build read-only worker
        let ro_worker = ReadOnlyWorker::new(path_str.to_string());

        Ok(ConfigAndPool {
            db_type: DatabaseType::Sqlite,
            pool: MiddlewarePool::Sqlite {
                read_only_worker: Arc::new(ro_worker),
                write_pool: Arc::new(write_pool),
            },
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
impl From<r2d2::Error> for DbError {
    fn from(err: r2d2::Error) -> Self {
        DbError::Other(format!("R2D2 Pool Error: {}", err))
    }
}

impl ReadOnlyWorker {
    /// Creates a new read-only worker thread for SQLite queries.
    pub fn new(db_path: String) -> Self {
        let (tx, rx): (Sender<ReadOnlyQuery>, Receiver<ReadOnlyQuery>) = std::sync::mpsc::channel();

        // Spawn the worker thread
        thread::spawn(move || {
            // Open the shared read-only connection
            let conn_result = Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            );
            let conn = match conn_result {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to open read-only SQLite connection: {}", e);
                    return;
                }
            };

            // Listen for incoming queries
            for read_only_query in rx {
                let ReadOnlyQuery {
                    query,
                    params,
                    response,
                } = read_only_query;

                let exec_res = Self::execute_query(&conn, &query, &params);
                let _ = response.send(exec_res);
            }
        });

        Self { sender: tx }
    }

    /// Executes a SELECT (read-only) query synchronously on the read-only `conn`.
    fn execute_query(
        conn: &Connection,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, DbError> {
        let bound_params_res = Self::convert_params(params);
        let bound_params = match bound_params_res {
            Ok(bp) => bp,
            Err(e) => return Err(e),
        };

        let stmt_res = conn.prepare(query);
        let mut stmt = match stmt_res {
            Ok(s) => s,
            Err(e) => return Err(DbError::SqliteError(e)),
        };

        let column_names = stmt
            .column_names()
            .iter()
            .map(|&c| c.to_string())
            .collect::<Vec<_>>();

        let mut result_set = ResultSet {
            results: vec![],
            rows_affected: 0,
        };

        let rows_res = stmt.query_map(rusqlite::params_from_iter(bound_params), |row| {
            let mut row_values = Vec::new();
            for (i, _col_name) in column_names.iter().enumerate() {
                let extracted = Self::sqlite_extract_value_sync(row, i);
                match extracted {
                    Ok(val) => {
                        row_values.push(val);
                    }
                    Err(e) => {
                        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(e)));
                    }
                }
            }
            Ok(CustomDbRow {
                column_names: column_names.clone(),
                rows: row_values,
            })
        });

        let rows_iter = match rows_res {
            Ok(r) => r,
            Err(e) => return Err(DbError::SqliteError(e)),
        };

        for row_result in rows_iter {
            match row_result {
                Ok(crow) => {
                    result_set.results.push(crow);
                }
                Err(e) => {
                    return Err(DbError::SqliteError(e));
                }
            }
        }
        result_set.rows_affected = result_set.results.len();

        Ok(result_set)
    }

    /// Convert our `RowValues` into rusqlite Values for binding.
    fn convert_params(params: &[RowValues]) -> Result<Vec<rusqlite::types::Value>, DbError> {
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

    /// Extract a single column value from a rusqlite Row by index.
    fn sqlite_extract_value_sync(row: &rusqlite::Row, idx: usize) -> Result<RowValues, DbError> {
        use rusqlite::types::ValueRef::*;
        match row.get_ref(idx)? {
            Null => Ok(RowValues::Null),
            Integer(i) => Ok(RowValues::Int(i)),
            Real(f) => Ok(RowValues::Float(f)),
            Text(bytes) => Ok(RowValues::Text(String::from_utf8_lossy(bytes).to_string())),
            Blob(b) => Ok(RowValues::Blob(b.to_vec())),
        }
    }
}

/// Executes a write-query synchronously using a write-connection.
pub fn exec_write_query_sync(
    conn: &Connection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, DbError> {
    let bound_params = convert_params_for_write(params)?;
    let mut stmt = conn.prepare(query)?;
    let rows_affected = stmt.execute(rusqlite::params_from_iter(bound_params))?;
    Ok(rows_affected)
}

/// A small helper for re-binding params specifically in the write context.
fn convert_params_for_write(params: &[RowValues]) -> Result<Vec<rusqlite::types::Value>, DbError> {
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
