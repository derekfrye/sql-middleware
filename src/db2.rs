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

impl CustomDbRow {
    pub fn get_column_index(&self, column_name: &str) -> Option<usize> {
        self.column_names.iter().position(|col| col == column_name)
    }

    pub fn get(&self, column_name: &str) -> Option<&RowValues> {
        let index = match self.get_column_index(column_name) {
            Some(idx) => idx,
            None => return None,
        };
        self.rows.get(index)
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
            let conn = match rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            ) {
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

                // Execute the query and collect results
                let result = Self::execute_query(&conn, &query, &params);

                // Send back the result
                let _ = response.send(result);
            }
        });

        Self { sender: tx }
    }

    fn execute_query(
        conn: &rusqlite::Connection,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, DbError> {
        let bound_params = match Self::convert_params(params) {
            Ok(bp) => bp,
            Err(e) => return Err(e),
        };

        let mut stmt = match conn.prepare(query) {
            Ok(s) => s,
            Err(e) => return Err(DbError::SqliteError(e)),
        };

        // Get column names outside of the closure
        let column_names = stmt
            .column_names()
            .iter()
            .map(|&c| c.to_string())
            .collect::<Vec<_>>();

        let mut result_set = ResultSet {
            results: vec![],
            rows_affected: 0,
        };

        let rows = match stmt.query_map(rusqlite::params_from_iter(bound_params), |row| {
            let mut row_values = Vec::new();
            for (i, _col_name) in column_names.iter().enumerate() {
                // Previously: let rv = Self::sqlite_extract_value_sync(row, i).unwrap();
                // We'll do error handling here explicitly:
                let rv = match Self::sqlite_extract_value_sync(row, i) {
                    Ok(val) => val,
                    Err(e) => {
                        // If an error occurs extracting a single row/column, we return early
                        // from the closure. The query_map will then yield an error.
                        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(e)));
                    }
                };
                row_values.push(rv);
            }

            Ok(CustomDbRow {
                column_names: column_names.clone(),
                rows: row_values,
            })
        }) {
            Ok(r) => r,
            Err(e) => return Err(DbError::SqliteError(e)),
        };

        for row_result in rows {
            let custom_row = match row_result {
                Ok(r) => r,
                Err(e) => return Err(DbError::SqliteError(e)),
            };
            result_set.results.push(custom_row);
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
                RowValues::Timestamp(dt) => match dt.to_sql() {
                    Ok(ToSqlOutput::Owned(v)) => v,
                    Ok(ToSqlOutput::Borrowed(v)) => v.into(),
                    Err(e) => return Err(DbError::SqliteError(e)),
                    _ => return Err(DbError::Other("Unexpected ToSqlOutput variant".to_string())),
                },
                RowValues::Null => rusqlite::types::Value::Null,
                RowValues::JSON(jsval) => {
                    // Store JSON as text
                    rusqlite::types::Value::Text(jsval.to_string())
                }
                RowValues::Blob(bytes) => rusqlite::types::Value::Blob(bytes.clone()),
            };
            vec_values.push(v);
        }
        Ok(vec_values)
    }

    fn sqlite_extract_value_sync(row: &rusqlite::Row, idx: usize) -> Result<RowValues, DbError> {
        match row.get_ref(idx) {
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
                // Configure Postgres using deadpool_postgres
                let pg_config = config
                    .create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls)
                    .expect("Failed to create deadpool_postgres pool");
                let pool = pg_config;

                ConfigAndPool {
                    pool: MiddlewarePool::Postgres(pool),
                    db_type,
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
                let write_pool = r2d2::Pool::builder().max_size(10).build(manager).unwrap();

                {
                    let conn = write_pool
                        .get()
                        .expect("Failed to get initial write connection");
                    // Run any trivial query to initialize the DB in read‐write mode
                    conn.execute("SELECT 1;", []).ok();
                    // Or possibly run a PRAGMA / or even create some DDL. Just enough to ensure
                    // the underlying DB handle is opened in R/W mode first.
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

    pub async fn exec_general_query(
        &self,
        queries: Vec<QueryAndParams>,
        expect_rows: bool,
    ) -> Result<DatabaseResult<Vec<ResultSet>>, DbError> {
        let mut final_result = DatabaseResult::<Vec<ResultSet>>::default();
        let mut result_sets = Vec::new();

        match &self.config_and_pool.pool {
            // -----------------------------------------------------------------
            // POSTGRES
            // -----------------------------------------------------------------
            MiddlewarePool::Postgres(pg_pool) => {
                // Get a client
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

                // Begin transaction
                let begin_res = client.execute("BEGIN", &[]).await;
                match begin_res {
                    Ok(_) => { /* success */ }
                    Err(e) => {
                        return Err(DbError::PostgresError(e));
                    }
                }

                // Process each query
                for q in &queries {
                    if q.is_read_only {
                        // Read-only
                        let stmt_res = client.prepare(&q.query).await;
                        let stmt = match stmt_res {
                            Ok(s) => s,
                            Err(e) => {
                                // Attempt rollback
                                let _ = client.execute("ROLLBACK", &[]).await;
                                return Err(DbError::PostgresError(e));
                            }
                        };

                        let bound_params = self.convert_params_pg(&q.params);
                        let rows_res = client.query(&stmt, &bound_params).await;
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
                                        let val = self.extract_pg_value(&row, col.name(), type_name);
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
                        // Write
                        let stmt_res = client.prepare(&q.query).await;
                        let stmt = match stmt_res {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = client.execute("ROLLBACK", &[]).await;
                                return Err(DbError::PostgresError(e));
                            }
                        };

                        let bound_params = self.convert_params_pg(&q.params);
                        // If we expect rows, use `query`; otherwise `execute`
                        if expect_rows {
                            let rows_res = client.query(&stmt, &bound_params).await;
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
                                            let val =
                                                self.extract_pg_value(&row, col.name(), type_name);
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
                            let exec_res = client.execute(&stmt, &bound_params).await;
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

                // Commit
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
            // SQLITE
            // -----------------------------------------------------------------
            MiddlewarePool::Sqlite {
                write_pool,
                read_only_worker,
            } => {
                // We handle *all* queries in a single blocking transaction for writes
                // If the query is read-only, we use the read-only worker as before.
                // If you want a single transaction for read+write combined on the same connection,
                // remove the read_only_worker usage below.
                let queries_clone = queries.clone();
                let write_pool_clone = Arc::clone(write_pool);

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

                        let begin_res = conn.execute("BEGIN TRANSACTION", []);
                        match begin_res {
                            Ok(_) => { /* success */ }
                            Err(e) => {
                                return Err(DbError::SqliteError(e));
                            }
                        }

                        let mut local_results = Vec::new();
                        for q in &queries_clone {
                            if q.is_read_only {
                                // If truly read-only, use the read-only worker in a blocking way:
                                // (Alternatively, you could do it in the same connection, up to you.)
                                let (tx, rx) = oneshot::channel();
                                let roq = ReadOnlyQuery {
                                    query: q.query.clone(),
                                    params: q.params.clone(),
                                    response: tx,
                                };
                                let send_res = read_only_worker.sender.send(roq);
                                if let Err(e) = send_res {
                                    // Attempt rollback
                                    let _ = conn.execute("ROLLBACK", []);
                                    return Err(DbError::Other(format!(
                                        "Failed sending read-only query: {}",
                                        e
                                    )));
                                }

                                // Wait for reply
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
                                // Non-read-only => pass to exec_write_query_sync
                                let rows_affected_res = Db::exec_write_query_sync(
                                    &conn,
                                    &q.query,
                                    &q.params,
                                );
                                match rows_affected_res {
                                    Ok(aff) => {
                                        let rs = ResultSet {
                                            results: vec![],
                                            rows_affected: aff,
                                        };
                                        local_results.push(rs);
                                    }
                                    Err(e) => {
                                        let _ = conn.execute("ROLLBACK", []);
                                        return Err(e);
                                    }
                                }
                            }
                        }

                        let commit_res = conn.execute("COMMIT", []);
                        match commit_res {
                            Ok(_) => Ok(local_results),
                            Err(e) => Err(DbError::SqliteError(e)),
                        }
                    })
                    .await;

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
                        return Err(DbError::Other(format!(
                            "Spawn blocking join error: {}",
                            e
                        )));
                    }
                }
            }
        }

        Ok(final_result)
    }

    /// Synchronous function to execute write queries
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

    /// Convert `RowValues` to a `Vec<rusqlite::types::Value>` for binding.
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
                RowValues::JSON(jsval) => {
                    // Store JSON as text
                    rusqlite::types::Value::Text(jsval.to_string())
                }
                RowValues::Blob(bytes) => rusqlite::types::Value::Blob(bytes.clone()),
            };
            vec_values.push(v);
        }
        Ok(vec_values)
    }

    fn convert_params_pg<'a>(
        &self,
        params: &'a [RowValues],
    ) -> Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)> {
        let mut bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        for param in params {
            match param {
                RowValues::Int(i) => bound_params.push(i),
                RowValues::Float(f) => bound_params.push(f),
                RowValues::Text(s) => bound_params.push(s),
                RowValues::Bool(b) => bound_params.push(b),
                RowValues::Timestamp(dt) => bound_params.push(dt),
                RowValues::Null => {
                    // push a None typed as i32
                    let none_val = &None::<i32>;
                    bound_params.push(none_val);
                }
                RowValues::JSON(jsval) => {
                    bound_params.push(jsval);
                }
                RowValues::Blob(bytes) => {
                    bound_params.push(bytes);
                }
            }
        }
        bound_params
    }

    fn extract_pg_value(
        &self,
        row: &tokio_postgres::Row,
        col_name: &str,
        type_name: &str,
    ) -> RowValues {
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

    /// Implement Postgres query execution methods
    async fn exec_pg_returning(
        &self,
        pg_pool: &Pool,
        queries: &[QueryAndParams],
        final_result: &mut DatabaseResult<Vec<ResultSet>>,
    ) -> Result<(), DbError> {
        for q in queries {
            let client = match pg_pool.get().await {
                Ok(c) => c,
                Err(e) => return Err(DbError::Other(e.to_string())),
            };

            let stmt = match client.prepare(&q.query).await {
                Ok(s) => s,
                Err(e) => return Err(DbError::PostgresError(e)),
            };

            let bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = q
                .params
                .iter()
                .map(|param| match param {
                    RowValues::Int(i) => i as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Float(f) => f as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Text(s) => s as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Bool(b) => b as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Timestamp(dt) => dt as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Null => {
                        &Option::<i32>::None as &(dyn tokio_postgres::types::ToSql + Sync)
                    }
                    RowValues::JSON(jsval) => jsval as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Blob(bytes) => bytes as &(dyn tokio_postgres::types::ToSql + Sync),
                })
                .collect();

            let rows = match client.query(&stmt, &bound_params).await {
                Ok(r) => r,
                Err(e) => return Err(DbError::PostgresError(e)),
            };

            let mut result_set = ResultSet {
                results: vec![],
                rows_affected: 0,
            };

            for row in rows {
                let column_names = row
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect::<Vec<_>>();

                let values = row
                    .columns()
                    .iter()
                    .map(|col| {
                        let type_info = col.type_().name();
                        match type_info {
                            "INT4" | "INT8" | "BIGINT" | "INTEGER" | "INT" => {
                                RowValues::Int(row.get::<&str, i64>(col.name()))
                            }
                            "TEXT" => RowValues::Text(row.get::<&str, String>(col.name())),
                            "BOOL" | "BOOLEAN" => {
                                RowValues::Bool(row.get::<&str, bool>(col.name()))
                            }
                            "TIMESTAMP" => {
                                let timestamp: chrono::NaiveDateTime = row.get(col.name());
                                RowValues::Timestamp(timestamp)
                            }
                            "FLOAT8" | "DOUBLE PRECISION" => {
                                RowValues::Float(row.get::<&str, f64>(col.name()))
                            }
                            _ => {
                                eprintln!("Unknown column type: {}", type_info);
                                RowValues::Null
                            }
                        }
                    })
                    .collect::<Vec<_>>();

                result_set.results.push(CustomDbRow {
                    column_names,
                    rows: values,
                });
            }

            result_set.rows_affected = result_set.results.len();

            final_result.return_result.push(result_set);
        }

        Ok(())
    }

    async fn exec_pg_nonreturning(
        &self,
        pg_pool: &Pool,
        queries: &[QueryAndParams],
        final_result: &mut DatabaseResult<Vec<ResultSet>>,
    ) -> Result<(), DbError> {
        for q in queries {
            let client = match pg_pool.get().await {
                Ok(c) => c,
                Err(e) => return Err(DbError::Other(e.to_string())),
            };

            let stmt = match client.prepare(&q.query).await {
                Ok(s) => s,
                Err(e) => return Err(DbError::PostgresError(e)),
            };

            let bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = q
                .params
                .iter()
                .map(|param| match param {
                    RowValues::Int(i) => i as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Float(f) => f as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Text(s) => s as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Bool(b) => b as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Timestamp(dt) => dt as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Null => {
                        &Option::<i32>::None as &(dyn tokio_postgres::types::ToSql + Sync)
                    }
                    RowValues::JSON(jsval) => jsval as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Blob(bytes) => bytes as &(dyn tokio_postgres::types::ToSql + Sync),
                })
                .collect();

            let rows_affected = match client.execute(&stmt, &bound_params).await {
                Ok(x) => x,
                Err(e) => return Err(DbError::PostgresError(e)),
            };

            // Push an empty result set as the query does not return rows
            final_result.return_result.push(ResultSet {
                results: vec![],
                rows_affected: rows_affected.try_into().unwrap(),
            });
        }

        Ok(())
    }
}
