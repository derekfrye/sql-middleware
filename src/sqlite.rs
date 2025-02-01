use deadpool_sqlite::rusqlite::Transaction;
use deadpool_sqlite::{Object, Pool};
use deadpool_sqlite::{rusqlite, Config as DeadpoolSqliteConfig, Runtime};
use rusqlite::types::Value;
use rusqlite::Statement;
use rusqlite::ToSql;
use async_trait::async_trait;
use crate::middleware::{
    ConfigAndPool, CustomDbRow, DatabaseType, DbError, MiddlewarePool, MiddlewarePoolConnection, ResultSet, RowValues, StatementExecutor, TransactionExecutor
};
use std::sync::Arc;

// influenced design: https://tedspence.com/investigating-rust-with-sqlite-53d1f9a41112, https://www.powersync.com/blog/sqlite-optimizations-for-ultra-high-performance



impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Sqlite using deadpool_sqlite
    pub async fn new_sqlite(db_path: String) -> Result<Self, DbError> {
        // Configure deadpool_sqlite
        let cfg: DeadpoolSqliteConfig = DeadpoolSqliteConfig::new(db_path.clone());

        // Create the pool
        let pool = cfg
            .create_pool(Runtime::Tokio1)
            .map_err(|e| DbError::Other(format!("Failed to create Deadpool SQLite pool: {}", e)))?;

        // Initialize the database (e.g., create tables)
        {
            let conn = pool.get().await.map_err(DbError::PoolErrorSqlite)?;
            let _res = conn
                .interact(|conn| {
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

impl From<deadpool_sqlite::InteractError> for DbError {
    fn from(err: deadpool_sqlite::InteractError) -> Self {
        DbError::Other(format!("Deadpool SQLite Interact Error: {}", err))
    }
}

/// Bind middleware params to SQLite types.
// #[allow(dead_code)]
pub fn convert_params(params: &[RowValues]) -> Result<Vec<rusqlite::types::Value>, DbError> {
    let mut vec_values = Vec::with_capacity(params.len());
    for p in params {
        let v = match p {
            RowValues::Int(i) => rusqlite::types::Value::Integer(*i),
            RowValues::Float(f) => rusqlite::types::Value::Real(*f),
            RowValues::Text(s) => rusqlite::types::Value::Text(s.clone()),
            RowValues::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
            RowValues::Timestamp(dt) => {
                let formatted = dt.format("%F %T%.f").to_string(); // Adjust precision as needed
                rusqlite::types::Value::Text(formatted)
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

/// Will only run select queries. I think it ignores other queries.
pub fn build_result_set(stmt: &mut Statement, params: &[Value]) -> Result<ResultSet, DbError> {
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    let mut rows_iter = stmt.query(&param_refs[..])?;
    let mut result_set = ResultSet {
        results: Vec::new(),
        rows_affected: 0,
    };

    while let Some(row) = rows_iter.next()? {
        let mut row_values = Vec::new();

        for (i, _col_name) in column_names.iter().enumerate() {
            let value = sqlite_extract_value_sync(row, i)?;
            row_values.push(value);
        }

        result_set.results.push(CustomDbRow {
            column_names: column_names.clone(),
            rows: row_values,
        });

        result_set.rows_affected += 1;
    }

    Ok(result_set)
}

pub async fn execute_batch(sqlite_client: &Object, query: &str) -> Result<(), DbError> {
    let query_owned = query.to_owned();

    // Use interact to run the blocking code in a separate thread.
    sqlite_client
        .interact(move |conn| {
            // Begin a transaction
            let tx = conn.transaction()?;

            // Execute the batch of queries
            tx.execute_batch(&query_owned)?;

            // Commit the transaction
            tx.commit()?;

            Ok(())
        })
        .await?
        .map_err(DbError::SqliteError) // Map the inner rusqlite::Error to DbError
}

pub async fn execute_select(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, DbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread.
    let result_set = sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let mut stmt = conn.prepare(&query_owned)?;

            // Execute the query
            let result_set = build_result_set(&mut stmt, &params_owned)?;

            Ok::<_, DbError>(result_set)
        })
        .await??;

    Ok(result_set)
}

pub async fn execute_dml(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, DbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread.
    let result_set = sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let tx = conn.transaction()?;
            // Execute the query
            let param_refs: Vec<&dyn ToSql> =
                params_owned.iter().map(|v| v as &dyn ToSql).collect();
            let rows = {
                let mut stmt = tx.prepare(&query_owned)?;
                stmt.execute(&param_refs[..])?
            };
            tx.commit()?;

            Ok::<_, DbError>(rows)
        })
        .await??;

    Ok(result_set)
}

#[async_trait]
impl<'a> TransactionExecutor<'a> for Transaction<'a> {
    async fn prepare(&mut self, query: &str) -> Result<Box<dyn StatementExecutor + Send + Sync + 'a>, DbError> {
        let stmt = self.prepare(query)?;
        Ok(Box::new(SqliteStatementExecutor { stmt }))
    }

    async fn execute(&mut self, query: &str, params: &[RowValues]) -> Result<usize, DbError> {
        // let params_converted = convert_params(params)?;
        let rows = self.execute(query, &params).await?;
        Ok(rows)
    }

    async fn batch_execute(&mut self, query: &str) -> Result<(), DbError> {
        self.execute_batch(query)?;
        Ok(())
    }

    async fn commit(&mut self) -> Result<(), DbError> {
        self.commit()?;
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), DbError> {
        self.rollback()?;
        Ok(())
    }
}

pub struct SqliteStatementExecutor {
    query: String,
    pool: Arc<Pool>,
}

impl SqliteStatementExecutor {
    pub fn new(query: &str, pool: Arc<Pool>) -> Self {
        Self {
            query: query.to_owned(),
            pool,
        }
    }
}

#[async_trait]
impl<'a> StatementExecutor for SqliteStatementExecutor<'a> {
    async fn execute(&mut self, params: &[RowValues]) -> Result<usize, DbError> {
        let query = self.query.clone();
        let params_converted = convert_params(params)?;
        let param_refs: Vec<&dyn ToSql> = params_converted.iter().map(|v| v as &dyn ToSql).collect();
        let pool = self.pool.clone();

        // Use `interact` to execute the blocking operation in a separate thread.
        let rows = pool.interact(move |conn| {
            // Begin a transaction
            let tx = conn.transaction()?;

            // Prepare the statement
            let mut stmt = tx.prepare(&query)?;

            // Execute the statement
            let rows = stmt.execute(&param_refs[..])?;

            // Commit the transaction
            tx.commit()?;

            Ok(rows)
        }).await??;

        Ok(rows)
    }

    async fn execute_select(&mut self, params: &[RowValues]) -> Result<ResultSet, DbError> {
        let query = self.query.clone();
        let params_converted = convert_params(params)?;
        let pool = self.pool.clone();

        // Use `interact` to execute the blocking operation in a separate thread.
        let result_set = pool.interact(move |conn| {
            // Begin a transaction
            let tx = conn.transaction()?;

            // Prepare the statement
            let mut stmt = tx.prepare(&query)?;

            // Execute the query and build the result set
            let result_set = build_result_set(&mut stmt, &params_converted,)?;

            // Commit the transaction
            tx.commit()?;

            Ok(result_set)
        }).await??;

        Ok(result_set)
    }
}

pub async fn begin_transaction<'a>(
    sqlite_client: &'a mut Object,
) -> Result<Box<dyn TransactionExecutor<'a> + Send + Sync>, DbError> {
    let tx = sqlite_client
        .interact(|conn| conn.transaction())
        .await?
        .map_err(DbError::SqliteError)?;
    Ok(Box::new(tx))
}

/// Prepares a statement for SQLite.
pub async fn prepare(
    sqlite_conn: &deadpool_sqlite::Object,
    query: &str,
) -> Result<Box<dyn StatementExecutor<'a> + Send + Sync>, DbError> {
    match sqlite_conn {
        MiddlewarePoolConnection::Sqlite { conn: _, pool } => {
            Ok(Box::new(SqliteStatementExecutor::new(query, pool.clone())))
        }
        _ => Err(DbError::Other("Not a Sqlite connection".to_string())),
    }
}

/// Executes a single query for SQLite.
pub async fn execute(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, DbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    let rows = sqlite_client
        .interact(move |conn| {
            // Begin a transaction
            let tx = conn.transaction()?;

            // Execute the query
            let rows = tx.execute(&query_owned, &params_owned)?;

            // Commit the transaction
            tx.commit()?;

            Ok::<_, DbError>(rows)
        })
        .await??;

    Ok(rows)
}

/// Executes a batch of queries for SQLite.
pub async fn batch_execute(
    sqlite_client: &SqliteObject,
    query: &str,
) -> Result<(), DbError> {
    let query_owned = query.to_owned();

    // Use interact to run the blocking code in a separate thread.
    sqlite_client
        .interact(move |conn| {
            // Begin a transaction
            let tx = conn.transaction()?;

            // Execute the batch of queries
            tx.execute_batch(&query_owned)?;

            // Commit the transaction
            tx.commit()?;

            Ok(())
        })
        .await?
        .map_err(DbError::SqliteError) // Map the inner rusqlite::Error to DbError
}