use std::sync::{Arc, mpsc::{self, Sender, Receiver}};
use std::thread;
use tokio::sync::oneshot;
use serde_json::Value as JsonValue;
use chrono::NaiveDateTime;
use deadpool_postgres::{Config,  Pool,};
use tokio_postgres::NoTls;
use r2d2::Pool as R2D2Pool;
use r2d2_sqlite::SqliteConnectionManager;

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
}

#[derive(Debug, Clone, Default)]
pub struct DatabaseResult<T> {
    pub return_result: T,
    pub db_last_exec_state: QueryState,
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
                let ReadOnlyQuery { query, params, response } = read_only_query;

                // Execute the query and collect results
                let result = Self::execute_query(&conn, &query, &params);

                // Send back the result
                let _ = response.send(result);
            }
        });

        Self { sender: tx }
    }

    fn execute_query(conn: &rusqlite::Connection, query: &str, params: &[RowValues]) -> Result<ResultSet, DbError> {
        let bound_params = Self::convert_params(params)?;
    
        let mut stmt = conn.prepare(query)?;
        
        // Get column names outside of the closure
        let column_names = stmt.column_names().iter().map(|&c| c.to_string()).collect::<Vec<_>>();
    
        let mut result_set = ResultSet { results: vec![] };
    
        let rows = stmt.query_map(rusqlite::params_from_iter(bound_params), |row| {
            let mut row_values = Vec::new();
            for (i, _col_name) in column_names.iter().enumerate() {
                let rv = Self::sqlite_extract_value_sync(row, i).unwrap();
                row_values.push(rv);
            }
    
            Ok(CustomDbRow {
                column_names: column_names.clone(),
                rows: row_values,
            })
        })?;
    
        for row_result in rows {
            let custom_row = row_result.map_err(DbError::SqliteError)?;
            result_set.results.push(custom_row);
        }
    
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
                config.dbname.as_ref().unwrap().clone() // SQLite uses the file path directly
            }
        };

        match db_type {
            DatabaseType::Postgres => {
                // Configure Postgres using deadpool_postgres
                let pg_config = config.create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls)
                    .expect("Failed to create deadpool_postgres pool");
                let pool = pg_config;

                ConfigAndPool {
                    pool: MiddlewarePool::Postgres(pool),
                    db_type,
                }
            }
            DatabaseType::Sqlite => {
                // Initialize the read-only worker
                let read_only_worker = ReadOnlyWorker::new(connection_string.clone());

                // Setup the write connection pool using r2d2
                let manager = SqliteConnectionManager::file(&connection_string)
                    .with_flags(rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE);
                // let config = r2d2::Config::default();
                let write_pool = r2d2::Pool::builder().max_size(10).build(manager).unwrap();
                    // .expect("Failed to create r2d2 Sqlite write pool");
                // R2D2Pool::new(config, manager)
                //    .expect("Failed to create r2d2 Sqlite write pool");

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

        // Debug block remains unchanged
        #[cfg(debug_assertions)]
        {
            // if !queries.is_empty()
            //     && !queries[0].params.is_empty()
            //     && queries[0].params[0].as_text().is_some()
            //     && queries[0].params[0].as_text().unwrap().contains("Player1")
            // {
            //     eprintln!("query about to run: {}", queries[0].query);
            // }
        }

        for q in queries {
            if q.is_read_only {
                // Handle read-only query
                match &self.config_and_pool.pool {
                    MiddlewarePool::Postgres(pg_pool) => {
                        if expect_rows {
                            self.exec_pg_returning(pg_pool, &[q], &mut final_result).await?;
                        } else {
                            self.exec_pg_nonreturning(pg_pool, &[q], &mut final_result).await?;
                        }
                    }
                    MiddlewarePool::Sqlite { read_only_worker, .. } => {
                        // Create a oneshot channel to receive the result
                        let (tx, rx) = oneshot::channel();

                        // Send the read-only query to the worker
                        let read_only_query = ReadOnlyQuery {
                            query: q.query.clone(),
                            params: q.params.clone(),
                            response: tx,
                        };

                        // Send the query
                        if let Err(e) = read_only_worker.sender.send(read_only_query) {
                            return Err(DbError::Other(format!("Failed to send read-only query: {}", e)));
                        }

                        // Await the response
                        match rx.await {
                            Ok(result) => match result {
                                Ok(result_set) => {
                                    final_result.return_result.push(result_set);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            },
                            Err(e) => {
                                return Err(DbError::Other(format!("Read-only query receiver error: {}", e)));
                            }
                        }
                    }
                }
            } else {
                // Handle write query
                match &self.config_and_pool.pool {
                    MiddlewarePool::Postgres(pg_pool) => {
                        if expect_rows {
                            self.exec_pg_returning(pg_pool, &[q], &mut final_result).await?;
                        } else {
                            self.exec_pg_nonreturning(pg_pool, &[q], &mut final_result).await?;
                        }
                    }
                    MiddlewarePool::Sqlite { write_pool, .. } => {
                        let query = q.query.clone();
                        let params = q.params.clone();

                        // Acquire a connection from the write pool
                        let write_pool = Arc::clone(write_pool);

                        // Spawn a blocking task to handle the write query
                        let write_result = tokio::task::spawn_blocking(move || -> Result<ResultSet, DbError> {
                            let conn = write_pool.get()?;
                            Self::exec_write_query_sync(&conn, &query, &params)?;
                            // For write queries, we assume no rows are returned
                            Ok(ResultSet { results: vec![] })
                        })
                        .await
                        .map_err(|e| DbError::Other(format!("Spawn blocking error: {}", e)))??;

                        final_result.return_result.push(write_result);
                    }
                }
            }
        }

        final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
        Ok(final_result)
    }

    /// Synchronous function to execute write queries
    fn exec_write_query_sync(
        conn: &rusqlite::Connection,
        query: &str,
        params: &[RowValues],
    ) -> Result<(), DbError> {
        let bound_params = Self::convert_params(params)?;

        let mut stmt = conn.prepare(query)?;
        stmt.execute(rusqlite::params_from_iter(bound_params))?;

        Ok(())
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

    /// Implement Postgres query execution methods
    async fn exec_pg_returning(
        &self,
        pg_pool: &Pool,
        queries: &[QueryAndParams],
        final_result: &mut DatabaseResult<Vec<ResultSet>>,
    ) -> Result<(), DbError> {
        for q in queries {
            let client = pg_pool.get().await.map_err(|e| DbError::Other(e.to_string()))?;

            let stmt = client.prepare(&q.query).await.map_err(DbError::PostgresError)?;

            let bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = q
                .params
                .iter()
                .map(|param| match param {
                    RowValues::Int(i) => i as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Float(f) => f as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Text(s) => s as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Bool(b) => b as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Timestamp(dt) => dt as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Null => &Option::<i32>::None as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::JSON(jsval) => jsval as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Blob(bytes) => bytes as &(dyn tokio_postgres::types::ToSql + Sync),
                })
                .collect();

            let rows = client.query(&stmt, &bound_params).await.map_err(DbError::PostgresError)?;

            let mut result_set = ResultSet { results: vec![] };

            for row in rows {
                let column_names = row.columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect::<Vec<_>>();

                let values = row.columns()
                    .iter()
                    .map(|col| {
                        let type_info = col.type_().name();
                        match type_info {
                            "INT4" | "INT8" | "BIGINT" | "INTEGER" | "INT" => {
                                RowValues::Int(row.get::<&str, i64>(col.name()))
                            }
                            "TEXT" => {
                                RowValues::Text(row.get::<&str, String>(col.name()))
                            }
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
            let client = pg_pool.get().await.map_err(|e| DbError::Other(e.to_string()))?;

            let stmt = client.prepare(&q.query).await.map_err(DbError::PostgresError)?;

            let bound_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = q
                .params
                .iter()
                .map(|param| match param {
                    RowValues::Int(i) => i as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Float(f) => f as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Text(s) => s as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Bool(b) => b as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Timestamp(dt) => dt as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Null => &Option::<i32>::None as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::JSON(jsval) => jsval as &(dyn tokio_postgres::types::ToSql + Sync),
                    RowValues::Blob(bytes) => bytes as &(dyn tokio_postgres::types::ToSql + Sync),
                })
                .collect();

            client.execute(&stmt, &bound_params).await.map_err(DbError::PostgresError)?;

            // Push an empty result set as the query does not return rows
            final_result.return_result.push(ResultSet { results: vec![] });
        }

        Ok(())
    }
}
