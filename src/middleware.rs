// dbx.rs

use std::fmt;
use std::error::Error;
use tokio::task::spawn_blocking;

use deadpool_postgres::Config as PgConfig;

use crate::db_model::{
    ConfigAndPool, CustomDbRow, DatabaseResult, DatabaseType, Db, DbError::{self}, MiddlewarePool, QueryAndParams, QueryState, ResultSet, RowValues
};

use crate::postgres::extract_pg_value;
use crate::sqlite::exec_write_query_sync;


// ----------------------------------------
// Common impl blocks for DbError
// ----------------------------------------
impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::PostgresError(e) => write!(f, "PostgresError: {}", e),
            DbError::SqliteError(e) => write!(f, "SqliteError: {}", e),
            DbError::Other(msg) => write!(f, "Other: {}", msg),
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
    pub fn new_auto(config: &PgConfig, db_type: DatabaseType) -> Result<Self, DbError> {
        match db_type {
            DatabaseType::Postgres => {
                // We need to clone, because `TryFrom<PgConfig>` consumes the input
                let config_clone = config.clone();
                config_clone.try_into()
            }
            DatabaseType::Sqlite => {
                // For SQLite, we interpret `config.dbname` as the file path
                let path = config.dbname
                    .clone()
                    .ok_or_else(|| DbError::Other("No dbname provided for SQLite".into()))?;

                path.try_into()
            }
        }
    }
}

// ----------------------------------------
// Impl for Db (the top-level "common" database wrapper)
// ----------------------------------------
impl Db {
    pub fn new(cnf: ConfigAndPool) -> Result<Self, String> {
        Ok(Self {
            config_and_pool: cnf,
        })
    }

    /// Executes one or more queries in a single transaction, unifying logic for Postgres and SQLite.
    pub async fn exec_general_query(
        &self,
        queries: Vec<QueryAndParams>,
        expect_rows: bool,
    ) -> Result<DatabaseResult<Vec<ResultSet>>, DbError> {
        let mut final_result = DatabaseResult::<Vec<ResultSet>>::default();
        let mut result_sets = Vec::new();

        match &self.config_and_pool.pool {
            // -----------------------
            // Postgres branch
            // -----------------------
            MiddlewarePool::Postgres(pg_pool) => {
                let client = pg_pool.get().await
                    .map_err(|e| DbError::Other(format!("Failed to get PG client: {}", e)))?;

                // BEGIN
                client.execute("BEGIN", &[]).await?;

                for q in &queries {
                    let stmt = client.prepare(&q.query).await?;
                    // Build references inline
                    let mut bound_params: Vec<&(dyn tokio_postgres::types::ToSql  + Sync)> = Vec::new();
                    for param in &q.params {
                        match param {
                            RowValues::Int(i) => bound_params.push(i),
                            RowValues::Float(f) => bound_params.push(f),
                            RowValues::Text(s) => bound_params.push(s),
                            RowValues::Bool(b) => bound_params.push(b),
                            RowValues::Timestamp(dt) => bound_params.push(dt),
                            RowValues::Null => {
                                let none_val = &None::<i32>;
                                bound_params.push(none_val);
                            }
                            RowValues::JSON(jsval) => bound_params.push(jsval),
                            RowValues::Blob(bytes) => bound_params.push(bytes),
                        }
                    }

                    if expect_rows {
                        let rows = client.query(&stmt, &bound_params[..]).await?;
                        let mut rs = ResultSet::default();
                        for row in rows {
                            let mut col_names = Vec::new();
                            let mut row_vals = Vec::new();
                            for col in row.columns() {
                                col_names.push(col.name().to_owned());
                                let type_name = col.type_().name();
                                let val = extract_pg_value(&row, col.name(), type_name);
                                row_vals.push(val);
                            }
                            rs.results.push(CustomDbRow {
                                column_names: col_names,
                                rows: row_vals,
                            });
                        }
                        rs.rows_affected = rs.results.len();
                        result_sets.push(rs);
                    } else {
                        // If we do not expect rows
                        let aff = client.execute(&stmt, &bound_params[..]).await?;
                        let mut rs = ResultSet::default();
                        rs.rows_affected = aff as usize;
                        result_sets.push(rs);
                    }
                }

                // COMMIT
                client.execute("COMMIT", &[]).await?;
                final_result.return_result = result_sets;
                final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
            }

            // -----------------------
            // SQLite branch
            // -----------------------
            MiddlewarePool::Sqlite { write_pool, read_only_worker } => {
                let queries_clone = queries.clone();
                let wp_clone = std::sync::Arc::clone(write_pool);
                let ro_worker_clone = std::sync::Arc::clone(read_only_worker);

                // We use spawn_blocking for synchronous access to the r2d2 handle
                let blocking_res = spawn_blocking(move || -> Result<Vec<ResultSet>, DbError> {
                    let conn = wp_clone.get()?;
                    conn.execute("BEGIN DEFERRED TRANSACTION;", [])?;

                    let mut local_results = Vec::new();
                    for q in &queries_clone {
                        if q.is_read_only {
                            // READ-ONLY => send to the read-only thread
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            let roq = crate::db_model::ReadOnlyQuery {
                                query: q.query.clone(),
                                params: q.params.clone(),
                                response: tx,
                            };
                            ro_worker_clone.sender
                                .send(roq)
                                .map_err(|e| DbError::Other(format!("Send read-only query: {}", e)))?;

                            // block until we get the result
                            let ro_result = rx.blocking_recv()
                                .map_err(|e| DbError::Other(format!("RO channel error: {}", e)))??;
                            local_results.push(ro_result);
                        } else {
                            // WRITE => call synchronous exec
                            let aff = exec_write_query_sync(&conn, &q.query, &q.params)?;
                            let mut rs = ResultSet::default();
                            rs.rows_affected = aff;
                            local_results.push(rs);
                        }
                    }

                    conn.execute("COMMIT;", [])?;
                    Ok(local_results)
                }).await;

                // Check the spawn_blocking result
                match blocking_res {
                    Ok(Ok(rsets)) => {
                        final_result.return_result = rsets;
                        final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                    }
                    Ok(Err(db_err)) => {
                        return Err(db_err);
                    }
                    Err(join_err) => {
                        return Err(DbError::Other(format!("Join error: {}", join_err)));
                    }
                }
            }
        }

        Ok(final_result)
    }
}
