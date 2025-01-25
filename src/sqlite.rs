use deadpool_sqlite::{rusqlite, Config as DeadpoolSqliteConfig, Runtime};
use rusqlite::types::Value;
use rusqlite::Statement;
use rusqlite::ToSql;

use crate::middleware::{
    ConfigAndPool, CustomDbRow, DatabaseType, DbError, MiddlewarePool, ResultSet, RowValues,
};

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
