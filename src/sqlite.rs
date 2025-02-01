use deadpool_sqlite::Object;
use deadpool_sqlite::{rusqlite, Config as DeadpoolSqliteConfig, Runtime};
use rusqlite::types::Value;
use rusqlite::Statement;
use rusqlite::ToSql;

use crate::middleware::{
    ConfigAndPool, CustomDbRow, DatabaseType, SqlMiddlewareDbError, MiddlewarePool, ResultSet, RowValues,
};

// influenced design: https://tedspence.com/investigating-rust-with-sqlite-53d1f9a41112, https://www.powersync.com/blog/sqlite-optimizations-for-ultra-high-performance

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Sqlite using deadpool_sqlite
    pub async fn new_sqlite(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        // Configure deadpool_sqlite
        let cfg: DeadpoolSqliteConfig = DeadpoolSqliteConfig::new(db_path.clone());

        // Create the pool
        let pool = cfg
            .create_pool(Runtime::Tokio1)
            .map_err(|e| SqlMiddlewareDbError::Other(format!("Failed to create Deadpool SQLite pool: {}", e)))?;

        // Initialize the database (e.g., create tables)
        {
            let conn = pool.get().await.map_err(SqlMiddlewareDbError::PoolErrorSqlite)?;
            let _res = conn
                .interact(|conn| {
                    conn.execute_batch(
                        "
                    PRAGMA journal_mode = WAL;
                ",
                    )
                    .map_err(|e| SqlMiddlewareDbError::SqliteError(e))
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
impl From<rusqlite::Error> for SqlMiddlewareDbError {
    fn from(err: rusqlite::Error) -> Self {
        SqlMiddlewareDbError::SqliteError(err)
    }
}

impl From<deadpool_sqlite::InteractError> for SqlMiddlewareDbError {
    fn from(err: deadpool_sqlite::InteractError) -> Self {
        SqlMiddlewareDbError::Other(format!("Deadpool SQLite Interact Error: {}", err))
    }
}

/// Bind middleware params to SQLite types.
pub fn convert_params(params: &[RowValues]) -> Result<Vec<rusqlite::types::Value>, SqlMiddlewareDbError> {
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

fn sqlite_extract_value_sync(row: &rusqlite::Row, idx: usize) -> Result<RowValues, SqlMiddlewareDbError> {
    let val_ref_res = row.get_ref(idx);
    match val_ref_res {
        Err(e) => Err(SqlMiddlewareDbError::SqliteError(e)),
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

/// Only SELECT queries return rows affected. If a DML is sent, it does run it. 
/// If there's more than one query in the statment, idk which statement will be run.
pub fn build_result_set(stmt: &mut Statement, params: &[Value]) -> Result<ResultSet, SqlMiddlewareDbError> {
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

pub async fn execute_batch(sqlite_client: &Object, query: &str) -> Result<(), SqlMiddlewareDbError> {
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
        .map_err(SqlMiddlewareDbError::SqliteError) // Map the inner rusqlite::Error to DbError
}

pub async fn execute_select(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread.
    let result_set = sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let mut stmt = conn.prepare(&query_owned)?;

            // Execute the query
            let result_set = build_result_set(&mut stmt, &params_owned)?;

            Ok::<_, SqlMiddlewareDbError>(result_set)
        })
        .await??;

    Ok(result_set)
}

pub async fn execute_dml(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
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

            Ok::<_, SqlMiddlewareDbError>(rows)
        })
        .await??;

    Ok(result_set)
}
