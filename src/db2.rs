use chrono::NaiveDateTime;
use deadpool_postgres::{Config, Pool};
use r2d2::Pool as R2D2Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{types::ToSqlOutput, ToSql};
use serde_json::Value as JsonValue;
use std::error::Error;
use std::thread::{self};
use std::{
    fmt,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
};
use tokio::sync::oneshot;
use tokio_postgres::NoTls;

// Define SqliteWritePool using r2d2
type SqliteWritePool = R2D2Pool<SqliteConnectionManager>;

// Define your custom structs and enums
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
    Postgres(Pool),
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

#[derive(Debug)]
pub enum DbError {
    PostgresError(tokio_postgres::Error),
    SqliteError(rusqlite::Error),
    Other(String),
}

impl From<tokio_postgres::Error> for DbError {
    fn from(err: tokio_postgres::Error) -> Self {
        DbError::PostgresError(err)
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        DbError::SqliteError(err)
    }
}

impl From<r2d2::Error> for DbError {
    fn from(err: r2d2::Error) -> Self {
        DbError::Other(format!("R2D2 Pool Error: {}", err))
    }
}

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

// ReadOnlyQuery and ReadOnlyWorker as defined earlier...
struct ReadOnlyQuery {
    query: String,
    params: Vec<RowValues>,
    response: oneshot::Sender<Result<ResultSet, DbError>>,
}

#[derive(Debug, Clone)]
pub struct ReadOnlyWorker {
    sender: Sender<ReadOnlyQuery>,
}

// Free function to extract a single column from a Postgres row into a RowValues
fn extract_pg_value(row: &tokio_postgres::Row, col_name: &str, type_name: &str) -> RowValues {
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
            Some(value)
        } else {
            None
        }
    }

    pub fn as_timestamp(&self) -> Option<&NaiveDateTime> {
        if let RowValues::Timestamp(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_json(&self) -> Option<&JsonValue> {
        if let RowValues::JSON(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_blob(&self) -> Option<&Vec<u8>> {
        if let RowValues::Blob(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        if let RowValues::Float(value) = self {
            Some(*value as f64)
        } else {
            None
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, RowValues::Null)
    }
}

impl ReadOnlyWorker {
    fn new(db_path: String) -> Self {
        let (tx, rx): (Sender<ReadOnlyQuery>, Receiver<ReadOnlyQuery>) = mpsc::channel();

        // Spawn the worker thread
        thread::spawn(move || {
            // Open the shared read-only connection
            let conn_result = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
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

    fn execute_query(
        conn: &rusqlite::Connection,
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

    fn sqlite_extract_value_sync(row: &rusqlite::Row, idx: usize) -> Result<RowValues, DbError> {
        let val_ref_res = row.get_ref(idx);
        match val_ref_res {
            Err(e) => Err(DbError::SqliteError(e)),
            Ok(rusqlite::types::ValueRef::Null) => Ok(RowValues::Null),
            Ok(rusqlite::types::ValueRef::Integer(i)) => Ok(RowValues::Int(i)),
            Ok(rusqlite::types::ValueRef::Real(f)) => Ok(RowValues::Float(f)),
            Ok(rusqlite::types::ValueRef::Text(bytes)) => {
                let s = String::from_utf8_lossy(bytes).into_owned();
                Ok(RowValues::Text(s))
            }
            Ok(rusqlite::types::ValueRef::Blob(b)) => Ok(RowValues::Blob(b.to_vec())),
        }
    }
}

impl ConfigAndPool {
    pub async fn new(config: &Config, db_type: DatabaseType) -> Self {
        // Instead of '?', we do manual checks
        if config.dbname.is_none() {
            panic!("dbname is required");
        }
        if db_type != DatabaseType::Sqlite {
            if config.host.is_none() {
                panic!("host is required");
            }
            if config.port.is_none() {
                panic!("port is required");
            }
            if config.user.is_none() {
                panic!("user is required");
            }
            if config.password.is_none() {
                panic!("password is required");
            }
        }

        let connection_string = match db_type {
            DatabaseType::Postgres => {
                format!(
                    "host={} port={} user={} password={} dbname={}",
                    config.host.as_ref().unwrap(),
                    config.port.as_ref().unwrap(),
                    config.user.as_ref().unwrap(),
                    config.password.as_ref().unwrap(),
                    config.dbname.as_ref().unwrap()
                )
            }
            DatabaseType::Sqlite => {
                // SQLite uses the file path directly
                config.dbname.as_ref().unwrap().clone()
            }
        };

        match db_type {
            DatabaseType::Postgres => {
                let pg_config_res =
                    config.create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls);
                match pg_config_res {
                    Ok(pg_pool) => ConfigAndPool {
                        pool: MiddlewarePool::Postgres(pg_pool),
                        db_type,
                    },
                    Err(e) => {
                        panic!("Failed to create deadpool_postgres pool: {}", e);
                    }
                }
            }
            DatabaseType::Sqlite => {
                // Setup the write connection pool using r2d2
                let manager = SqliteConnectionManager::file(&connection_string).with_flags(
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                        | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                        | rusqlite::OpenFlags::SQLITE_OPEN_URI,
                );
                // Build r2d2 pool
                let write_pool_res = r2d2::Pool::builder().max_size(10).build(manager);
                let write_pool = match write_pool_res {
                    Ok(p) => p,
                    Err(e) => {
                        panic!("Failed building r2d2 pool for SQLite: {}", e);
                    }
                };

                {
                    let conn_res = write_pool.get();
                    if let Ok(conn) = conn_res {
                        // Run any trivial query to initialize the DB in read‐write mode
                        let _ = conn.execute("SELECT 1;", []);
                    }
                }

                // Initialize the read-only worker
                let read_only_worker = ReadOnlyWorker::new(connection_string.clone());

                ConfigAndPool {
                    pool: MiddlewarePool::Sqlite {
                        read_only_worker: Arc::new(read_only_worker),
                        write_pool: Arc::new(write_pool),
                    },
                    db_type,
                }
            }
        }
    }
}

impl Db {
    pub fn new(cnf: ConfigAndPool) -> Result<Self, String> {
        Ok(Self {
            config_and_pool: cnf,
        })
    }

    /// -------------------------------------------------------------------------
    /// Rewrite of `exec_general_query` using single transactions + inline binding
    /// -------------------------------------------------------------------------
    pub async fn exec_general_query(
        &self,
        queries: Vec<QueryAndParams>,
        expect_rows: bool,
    ) -> Result<DatabaseResult<Vec<ResultSet>>, DbError> {
        let mut final_result = DatabaseResult::<Vec<ResultSet>>::default();
        let mut result_sets = Vec::new();

        match &self.config_and_pool.pool {
            // -----------------------------------------------------------------
            // POSTGRES SINGLE TRANSACTION
            // -----------------------------------------------------------------
            MiddlewarePool::Postgres(pg_pool) => {
                let client_res = pg_pool.get().await;
                let client = match client_res {
                    Ok(c) => c,
                    Err(e) => {
                        return Err(DbError::Other(format!(
                            "Failed to get PG client from pool: {}",
                            e
                        )));
                    }
                };

                // BEGIN
                let begin_res = client.execute("BEGIN", &[]).await;
                match begin_res {
                    Ok(_) => { /* success */ }
                    Err(e) => {
                        return Err(DbError::PostgresError(e));
                    }
                }

                // Process each query in a single transaction
                for q in &queries {
                    // Prepare the statement
                    let stmt_res = client.prepare(&q.query).await;
                    let stmt = match stmt_res {
                        Ok(s) => s,
                        Err(e) => {
                            let _ = client.execute("ROLLBACK", &[]).await;
                            return Err(DbError::PostgresError(e));
                        }
                    };

                    // Build references inline
                    let mut bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                        Vec::new();
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

                    if q.is_read_only {
                        // Use `query(...)` for read-only
                        let rows_res = client.query(&stmt, &bound_params[..]).await;
                        match rows_res {
                            Ok(rows) => {
                                let mut rs = ResultSet {
                                    results: vec![],
                                    rows_affected: 0,
                                };
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
                            }
                            Err(e) => {
                                let _ = client.execute("ROLLBACK", &[]).await;
                                return Err(DbError::PostgresError(e));
                            }
                        }
                    } else {
                        // Non-read (write) queries
                        if expect_rows {
                            // Use query(..) if you want to fetch data
                            let rows_res = client.query(&stmt, &bound_params[..]).await;
                            match rows_res {
                                Ok(rows) => {
                                    let mut rs = ResultSet {
                                        results: vec![],
                                        rows_affected: 0,
                                    };
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
                                }
                                Err(e) => {
                                    let _ = client.execute("ROLLBACK", &[]).await;
                                    return Err(DbError::PostgresError(e));
                                }
                            }
                        } else {
                            // Use execute(..) if we don't expect rows returned
                            let exec_res = client.execute(&stmt, &bound_params[..]).await;
                            match exec_res {
                                Ok(rows_affected) => {
                                    let rs = ResultSet {
                                        results: vec![],
                                        rows_affected: rows_affected as usize,
                                    };
                                    result_sets.push(rs);
                                }
                                Err(e) => {
                                    let _ = client.execute("ROLLBACK", &[]).await;
                                    return Err(DbError::PostgresError(e));
                                }
                            }
                        }
                    }
                }

                // COMMIT
                let commit_res = client.execute("COMMIT", &[]).await;
                match commit_res {
                    Ok(_) => {
                        final_result.return_result = result_sets;
                        final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                    }
                    Err(e) => {
                        let _ = client.execute("ROLLBACK", &[]).await;
                        return Err(DbError::PostgresError(e));
                    }
                }
            }

            // -----------------------------------------------------------------
            // SQLITE SINGLE TRANSACTION
            // -----------------------------------------------------------------
            MiddlewarePool::Sqlite {
                write_pool,
                read_only_worker,
            } => {
                // We handle all queries in a single blocking transaction for writes.
                // If the query is read-only, we still route it to read_only_worker.
                let queries_clone = queries.clone();
                let write_pool_clone = Arc::clone(write_pool);
                let ro_worker_clone = read_only_worker.clone();

                // Run blocking so we can do the transaction
                let blocking_res =
                    tokio::task::spawn_blocking(move || -> Result<Vec<ResultSet>, DbError> {
                        let conn_res = write_pool_clone.get();
                        let conn = match conn_res {
                            Ok(c) => c,
                            Err(e) => {
                                return Err(DbError::Other(format!(
                                    "Failed to get sqlite write-conn: {}",
                                    e
                                )));
                            }
                        };

                        let begin_res = conn.execute("BEGIN DEFERRED TRANSACTION;", []);
                        match begin_res {
                            Ok(_) => {}
                            Err(e) => {
                                return Err(DbError::SqliteError(e));
                            }
                        }

                        let mut local_results = Vec::new();
                        for q in &queries_clone {
                            if q.is_read_only {
                                // Send to read-only worker
                                let (tx, rx) = oneshot::channel();
                                let roq = ReadOnlyQuery {
                                    query: q.query.clone(),
                                    params: q.params.clone(),
                                    response: tx,
                                };
                                let send_res = ro_worker_clone.sender.send(roq);
                                if let Err(e) = send_res {
                                    let _ = conn.execute("ROLLBACK", []);
                                    return Err(DbError::Other(format!(
                                        "Failed sending read-only query: {}",
                                        e
                                    )));
                                }

                                let recv_res = rx.blocking_recv();
                                match recv_res {
                                    Ok(res) => match res {
                                        Ok(rs) => {
                                            local_results.push(rs);
                                        }
                                        Err(e) => {
                                            let _ = conn.execute("ROLLBACK", []);
                                            return Err(e);
                                        }
                                    },
                                    Err(e) => {
                                        let _ = conn.execute("ROLLBACK", []);
                                        return Err(DbError::Other(format!(
                                            "Read-only channel error: {}",
                                            e
                                        )));
                                    }
                                }
                            } else {
                                // Write query => pass to exec_write_query_sync
                                let rows_affected_res =
                                    Db::exec_write_query_sync(&conn, &q.query, &q.params);
                                match rows_affected_res {
                                    Ok(aff) => {
                                        let rs = ResultSet {
                                            results: vec![],
                                            rows_affected: aff,
                                        };
                                        local_results.push(rs);
                                    }
                                    Err(e) => {
                                        let _ = conn.execute("ROLLBACK;", []);
                                        return Err(e);
                                    }
                                }
                            }
                        }

                        let commit_res = conn.execute("COMMIT;", []);
                        match commit_res {
                            Ok(_) => Ok(local_results),
                            Err(e) => Err(DbError::SqliteError(e)),
                        }
                    })
                    .await;

                // Now handle the result
                match blocking_res {
                    Ok(inner_res) => match inner_res {
                        Ok(rsets) => {
                            final_result.return_result = rsets;
                            final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    },
                    Err(e) => {
                        return Err(DbError::Other(format!("Spawn blocking join error: {}", e)));
                    }
                }
            }
        }

        Ok(final_result)
    }

    /// Synchronous function to execute write queries (non-read) in SQLite
    pub fn exec_write_query_sync(
        conn: &rusqlite::Connection,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, DbError> {
        let bound_params_res = Self::convert_params(params);
        let bound_params = match bound_params_res {
            Ok(bp) => bp,
            Err(e) => {
                return Err(e);
            }
        };

        let stmt_res = conn.prepare(query);
        let mut stmt = match stmt_res {
            Ok(s) => s,
            Err(e) => {
                return Err(DbError::SqliteError(e));
            }
        };

        let exec_res = stmt.execute(rusqlite::params_from_iter(bound_params));
        match exec_res {
            Ok(rows_affected) => Ok(rows_affected),
            Err(e) => Err(DbError::SqliteError(e)),
        }
    }

    /// Convert `RowValues` to a `Vec<rusqlite::types::Value>` for binding (SQLite).
    fn convert_params(params: &[RowValues]) -> Result<Vec<rusqlite::types::Value>, DbError> {
        let mut vec_values = Vec::with_capacity(params.len());
        for p in params {
            let v = match p {
                RowValues::Int(i) => rusqlite::types::Value::Integer(*i),
                RowValues::Float(f) => rusqlite::types::Value::Real(*f),
                RowValues::Text(s) => rusqlite::types::Value::Text(s.clone()),
                RowValues::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
                RowValues::Timestamp(dt) => {
                    // Store as string "YYYY-MM-DD HH:MM:SS"
                    let s = dt.format("%Y-%m-%d %H:%M:%S").to_string();
                    rusqlite::types::Value::Text(s)
                }
                RowValues::Null => rusqlite::types::Value::Null,
                RowValues::JSON(jsval) => rusqlite::types::Value::Text(jsval.to_string()),
                RowValues::Blob(bytes) => rusqlite::types::Value::Blob(bytes.clone()),
            };
            vec_values.push(v);
        }
        Ok(vec_values)
    }
}
