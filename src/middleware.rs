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

// ----------------------------------------
// Impl for Db (the top-level "common" database wrapper)
// ----------------------------------------
// impl Db {
//     pub fn new(cnf: ConfigAndPool) -> Result<Self, String> {
//         Ok(Self {
//             config_and_pool: cnf,
//         })
//     }

//     /// Executes one or more queries in a single transaction, unifying logic for Postgres and SQLite.
//     pub async fn exec_general_query(
//         &self,
//         queries: Vec<QueryAndParams>,
//         expect_rows: bool,
//     ) -> Result<DatabaseResult<Vec<ResultSet>>, DbError> {
//         let mut final_result = DatabaseResult::<Vec<ResultSet>>::default();
//         let mut result_sets = Vec::new();

//         match &self.config_and_pool.pool {
//             // -----------------------
//             // Postgres branch
//             // -----------------------
//             MiddlewarePool::Postgres(pg_pool) => {
//                 let client_res = pg_pool.get().await;
//                 let client = match client_res {
//                     Ok(c) => c,
//                     Err(e) => {
//                         return Err(DbError::Other(format!(
//                             "Failed to get PG client from pool: {}",
//                             e
//                         )));
//                     }
//                 };

//                 // BEGIN
//                 let begin_res = client.execute("BEGIN", &[]).await;
//                 match begin_res {
//                     Ok(_) => { /* success */ }
//                     Err(e) => {
//                         return Err(DbError::PostgresError(e));
//                     }
//                 }

//                 for q in &queries {
//                     let stmt_res = client.prepare(&q.query).await;
//                     let stmt = match stmt_res {
//                         Ok(s) => s,
//                         Err(e) => {
//                             let _ = client.execute("ROLLBACK", &[]).await;
//                             return Err(DbError::PostgresError(e));
//                         }
//                     };
//                     // Build references inline
//                     let mut bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
//                         Vec::new();
//                     for param in &q.params {
//                         match param {
//                             RowValues::Int(i) => bound_params.push(i),
//                             RowValues::Float(f) => bound_params.push(f),
//                             RowValues::Text(s) => bound_params.push(s),
//                             RowValues::Bool(b) => bound_params.push(b),
//                             RowValues::Timestamp(dt) => bound_params.push(dt),
//                             RowValues::Null => {
//                                 let none_val = &None::<i32>;
//                                 bound_params.push(none_val);
//                             }
//                             RowValues::JSON(jsval) => bound_params.push(jsval),
//                             RowValues::Blob(bytes) => bound_params.push(bytes),
//                         }
//                     }

//                     if expect_rows {
//                         let rows_res = client.query(&stmt, &bound_params[..]).await;
//                         match rows_res {
//                             Ok(rows) => {
//                                 let mut rs = ResultSet {
//                                     results: vec![],
//                                     rows_affected: 0,
//                                 };
//                                 for row in rows {
//                                     let mut col_names = Vec::new();
//                                     let mut row_vals = Vec::new();
//                                     for col in row.columns() {
//                                         col_names.push(col.name().to_owned());
//                                         let type_name = col.type_().name();
//                                         let val = extract_pg_value(&row, col.name(), type_name);
//                                         row_vals.push(val);
//                                     }
//                                     rs.results.push(CustomDbRow {
//                                         column_names: col_names,
//                                         rows: row_vals,
//                                     });
//                                 }
//                                 rs.rows_affected = rs.results.len();
//                                 result_sets.push(rs);
//                             }
//                             Err(e) => {
//                                 let _ = client.execute("ROLLBACK", &[]).await;
//                                 return Err(DbError::PostgresError(e));
//                             }
//                         }
//                     } else {
//                         // Use execute(..) if we don't expect rows returned
//                         let exec_res = client.execute(&stmt, &bound_params[..]).await;
//                         match exec_res {
//                             Ok(rows_affected) => {
//                                 let rs = ResultSet {
//                                     results: vec![],
//                                     rows_affected: rows_affected as usize,
//                                 };
//                                 result_sets.push(rs);
//                             }
//                             Err(e) => {
//                                 let _ = client.execute("ROLLBACK", &[]).await;
//                                 return Err(DbError::PostgresError(e));
//                             }
//                         }
//                     }
//                 }

//                 // COMMIT
//                 let commit_res = client.execute("COMMIT", &[]).await;
//                 match commit_res {
//                     Ok(_) => {
//                         final_result.return_result = result_sets;
//                         final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
//                     }
//                     Err(e) => {
//                         let _ = client.execute("ROLLBACK", &[]).await;
//                         return Err(DbError::PostgresError(e));
//                     }
//                 }
//             }

//             // -----------------------
//             // SQLite branch
//             // -----------------------
//             MiddlewarePool::Sqlite {pool
//             } => {
//                 // We handle all queries in a single blocking transaction for writes.
//                 // If the query is read-only, we still route it to read_only_worker.
//                 let queries_clone = queries.clone();
//                 let write_pool_clone = Arc::clone(write_pool);
//                 let ro_worker_clone = read_only_worker.clone();

//                 // Run blocking so we can do the transaction
//                 let blocking_res =
//                     tokio::task::spawn_blocking(move || -> Result<Vec<ResultSet>, DbError> {
//                         let conn_res = write_pool_clone.get();
//                         let conn = match conn_res {
//                             Ok(c) => c,
//                             Err(e) => {
//                                 return Err(DbError::Other(format!(
//                                     "Failed to get sqlite write-conn: {}",
//                                     e
//                                 )));
//                             }
//                         };

//                         let begin_res = conn.execute("BEGIN DEFERRED TRANSACTION;", []);
//                         match begin_res {
//                             Ok(_) => {}
//                             Err(e) => {
//                                 return Err(DbError::SqliteError(e));
//                             }
//                         }

//                         let mut local_results = Vec::new();
//                         for q in &queries_clone {
//                             if q.is_read_only {
//                                 // Send to read-only worker
//                                 let (tx, rx) = oneshot::channel();
//                                 let roq = ReadOnlyQuery {
//                                     query: q.query.clone(),
//                                     params: q.params.clone(),
//                                     response: tx,
//                                 };
//                                 let send_res = ro_worker_clone.sender.send(roq);
//                                 if let Err(e) = send_res {
//                                     let _ = conn.execute("ROLLBACK", []);
//                                     return Err(DbError::Other(format!(
//                                         "Failed sending read-only query: {}",
//                                         e
//                                     )));
//                                 }

//                                 let recv_res = rx.blocking_recv();
//                                 match recv_res {
//                                     Ok(res) => match res {
//                                         Ok(rs) => {
//                                             local_results.push(rs);
//                                         }
//                                         Err(e) => {
//                                             let _ = conn.execute("ROLLBACK", []);
//                                             return Err(e);
//                                         }
//                                     },
//                                     Err(e) => {
//                                         let _ = conn.execute("ROLLBACK", []);
//                                         return Err(DbError::Other(format!(
//                                             "Read-only channel error: {}",
//                                             e
//                                         )));
//                                     }
//                                 }
//                             } else {
//                                 // Write query => pass to exec_write_query_sync
//                                 let rows_affected_res =
//                                     exec_write_query_sync(&conn, &q.query, &q.params);
//                                 match rows_affected_res {
//                                     Ok(aff) => {
//                                         let rs = ResultSet {
//                                             results: vec![],
//                                             rows_affected: aff,
//                                         };
//                                         local_results.push(rs);
//                                     }
//                                     Err(e) => {
//                                         let _ = conn.execute("ROLLBACK;", []);
//                                         return Err(e);
//                                     }
//                                 }
//                             }
//                         }

//                         let commit_res = conn.execute("COMMIT;", []);
//                         match commit_res {
//                             Ok(_) => Ok(local_results),
//                             Err(e) => Err(DbError::SqliteError(e)),
//                         }
//                     })
//                     .await;

//                 // Now handle the result
//                 match blocking_res {
//                     Ok(inner_res) => match inner_res {
//                         Ok(rsets) => {
//                             final_result.return_result = rsets;
//                             final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
//                         }
//                         Err(e) => {
//                             return Err(e);
//                         }
//                     },
//                     Err(e) => {
//                         return Err(DbError::Other(format!("Spawn blocking join error: {}", e)));
//                     }
//                 }
//             }
//         }

//         Ok(final_result)
//     }
// }
