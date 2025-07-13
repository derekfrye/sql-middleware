use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite::ParamsFromIter;
use deadpool_sqlite::{Config as DeadpoolSqliteConfig, Runtime, rusqlite};
use rusqlite::Statement;
use rusqlite::ToSql;
use rusqlite::types::Value;

use crate::middleware::{
    ConfigAndPool, ConversionMode, DatabaseType, MiddlewarePool, ParamConverter, ResultSet,
    RowValues, SqlMiddlewareDbError,
};

// influenced design: https://tedspence.com/investigating-rust-with-sqlite-53d1f9a41112, https://www.powersync.com/blog/sqlite-optimizations-for-ultra-high-performance

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Sqlite using deadpool_sqlite
    pub async fn new_sqlite(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        // Configure deadpool_sqlite
        let cfg: DeadpoolSqliteConfig = DeadpoolSqliteConfig::new(db_path.clone());

        // Create the pool
        let pool = cfg.create_pool(Runtime::Tokio1).map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQLite pool: {}", e))
        })?;

        // Initialize the database (e.g., create tables)
        {
            let conn = pool
                .get()
                .await
                .map_err(SqlMiddlewareDbError::PoolErrorSqlite)?;
            let _res = conn
                .interact(|conn| {
                    conn.execute_batch(
                        "
                    PRAGMA journal_mode = WAL;
                ",
                    )
                    .map_err(SqlMiddlewareDbError::SqliteError)
                })
                .await?;
        }

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Sqlite(pool),
            db_type: DatabaseType::Sqlite,
        })
    }
}

// The #[from] attribute on the SqlMiddlewareDbError::SqliteError variant
// automatically generates this implementation

/// Convert InteractError to a more specific SqlMiddlewareDbError
impl From<deadpool_sqlite::InteractError> for SqlMiddlewareDbError {
    fn from(err: deadpool_sqlite::InteractError) -> Self {
        SqlMiddlewareDbError::ConnectionError(format!("SQLite Interact Error: {}", err))
    }
}

// Thread-local buffer for efficient timestamp formatting
thread_local! {
    static TIMESTAMP_BUF: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(32));
}

// We'll remove this abstraction since it introduces complexity
// and just inline the param_refs code directly

/// Convert a single RowValue to a rusqlite Value
fn row_value_to_sqlite_value(value: &RowValues, for_execute: bool) -> rusqlite::types::Value {
    match value {
        RowValues::Int(i) => rusqlite::types::Value::Integer(*i),
        RowValues::Float(f) => rusqlite::types::Value::Real(*f),
        RowValues::Text(s) => {
            if for_execute {
                // For execute, we can move the owned String directly
                rusqlite::types::Value::Text(s.clone())
            } else {
                // For queries, we need to clone
                rusqlite::types::Value::Text(s.clone())
            }
        }
        RowValues::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
        // Format timestamps once for better performance
        RowValues::Timestamp(dt) => {
            // Use a thread_local buffer for timestamp formatting to avoid allocation
            TIMESTAMP_BUF.with(|buf| {
                let mut borrow = buf.borrow_mut();
                borrow.clear();
                // Format directly into the string buffer
                use std::fmt::Write;
                write!(borrow, "{}", dt.format("%F %T%.f")).unwrap();
                rusqlite::types::Value::Text(borrow.clone())
            })
        }
        RowValues::Null => rusqlite::types::Value::Null,
        RowValues::JSON(jval) => {
            // Only serialize once to avoid multiple allocations
            let json_str = jval.to_string();
            rusqlite::types::Value::Text(json_str)
        }
        RowValues::Blob(bytes) => {
            if for_execute {
                // For execute, we can directly use the bytes
                rusqlite::types::Value::Blob(bytes.clone())
            } else {
                // For queries, we need to clone
                rusqlite::types::Value::Blob(bytes.to_vec())
            }
        }
    }
}

/// Bind middleware params to SQLite types.
pub fn convert_params(
    params: &[RowValues],
) -> Result<Vec<rusqlite::types::Value>, SqlMiddlewareDbError> {
    let mut vec_values = Vec::with_capacity(params.len());
    for p in params {
        vec_values.push(row_value_to_sqlite_value(p, false));
    }
    Ok(vec_values)
}

/// Convert parameters for execution operations
pub fn convert_params_for_execute<I>(
    iter: I,
) -> Result<ParamsFromIter<std::vec::IntoIter<Value>>, SqlMiddlewareDbError>
where
    I: IntoIterator<Item = RowValues>,
{
    // Convert directly to avoid unnecessary collection
    let mut values = Vec::new();
    for p in iter {
        values.push(row_value_to_sqlite_value(&p, true));
    }
    // Note: clippy suggests removing .into_iter(), but removing it causes
    // a type error. This is because params_from_iter needs IntoIter<Value>,
    // not Vec<Value>, despite the type parameter suggesting otherwise.
    #[allow(clippy::useless_conversion)]
    Ok(rusqlite::params_from_iter(values.into_iter()))
}

/// Wrapper for SQLite parameters for queries.
pub struct SqliteParamsQuery(pub Vec<rusqlite::types::Value>);

/// Wrapper for SQLite parameters for execution.
pub struct SqliteParamsExecute(
    pub rusqlite::ParamsFromIter<std::vec::IntoIter<rusqlite::types::Value>>,
);

impl ParamConverter<'_> for SqliteParamsQuery {
    type Converted = Self;

    fn convert_sql_params(
        params: &[RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        match mode {
            // For a query, use the conversion that returns a Vec<Value>
            ConversionMode::Query => convert_params(params).map(SqliteParamsQuery),
            // Or, if you really want to support execution mode with this type, you might decide how to handle it:
            ConversionMode::Execute => {
                // For example, you could also call the "query" conversion here or return an error.
                convert_params(params).map(SqliteParamsQuery)
            }
        }
    }

    fn supports_mode(mode: ConversionMode) -> bool {
        // This converter is primarily for query operations
        mode == ConversionMode::Query
    }
}

impl ParamConverter<'_> for SqliteParamsExecute {
    type Converted = Self;

    fn convert_sql_params(
        params: &[RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        match mode {
            ConversionMode::Execute => {
                convert_params_for_execute(params.to_vec()).map(SqliteParamsExecute)
            }
            // For queries you might not support the "execute" wrapper:
            ConversionMode::Query => Err(SqlMiddlewareDbError::ParameterError(
                "SqliteParamsExecute can only be used with Execute mode".into(),
            )),
        }
    }

    fn supports_mode(mode: ConversionMode) -> bool {
        // This converter is only for execution operations
        mode == ConversionMode::Execute
    }
}

/// Extract a RowValues from a SQLite row
fn sqlite_extract_value_sync(
    row: &rusqlite::Row,
    idx: usize,
) -> Result<RowValues, SqlMiddlewareDbError> {
    let val_ref_res = row.get_ref(idx);
    match val_ref_res {
        Err(e) => Err(SqlMiddlewareDbError::SqliteError(e)),
        Ok(rusqlite::types::ValueRef::Null) => Ok(RowValues::Null),
        Ok(rusqlite::types::ValueRef::Integer(i)) => Ok(RowValues::Int(i)),
        Ok(rusqlite::types::ValueRef::Real(f)) => Ok(RowValues::Float(f)),
        Ok(rusqlite::types::ValueRef::Text(bytes)) => {
            // Fast path for ASCII (which most database text likely is)
            if bytes.is_ascii() {
                // ASCII is valid UTF-8, use lossy conversion for safety
                let s = String::from_utf8_lossy(bytes).into_owned();
                Ok(RowValues::Text(s))
            } else {
                // Fallback for non-ASCII
                let s = String::from_utf8_lossy(bytes).into_owned();
                Ok(RowValues::Text(s))
            }
        }
        Ok(rusqlite::types::ValueRef::Blob(b)) => Ok(RowValues::Blob(b.to_vec())),
    }
}

/// Build a result set from a SQLite query
/// Only SELECT queries return rows affected. If a DML is sent, it does run it.
/// If there's more than one query in the statement, idk which statement will be run.
pub fn build_result_set(
    stmt: &mut Statement,
    params: &[Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    // Store column names once in the result set
    let column_names_rc = std::sync::Arc::new(column_names);

    let mut rows_iter = stmt.query(&param_refs[..])?;
    // Create result set with default capacity
    let mut result_set = ResultSet::with_capacity(10);
    result_set.set_column_names(column_names_rc);

    while let Some(row) = rows_iter.next()? {
        let mut row_values = Vec::new();

        let col_count = result_set
            .get_column_names()
            .ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError("No column names available".to_string())
            })?
            .len();

        for i in 0..col_count {
            let value = sqlite_extract_value_sync(row, i)?;
            row_values.push(value);
        }

        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Execute a batch of SQL statements for SQLite
pub async fn execute_batch(
    sqlite_client: &Object,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
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
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Interact error: {}", e)))?
}

/// Execute a SELECT query in SQLite
pub async fn execute_select(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread and return the result directly
    sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let mut stmt = conn.prepare(&query_owned)?;

            // Execute the query
            build_result_set(&mut stmt, &params_owned)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Interact error: {}", e)))?
}

/// Execute a DML query (INSERT, UPDATE, DELETE) in SQLite
pub async fn execute_dml(
    sqlite_client: &Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let query_owned = query.to_owned();
    let params_owned = convert_params(params)?;

    // Use interact to run the blocking code in a separate thread.
    sqlite_client
        .interact(move |conn| {
            // Prepare the query
            let tx = conn.transaction()?;

            let param_refs: Vec<&dyn ToSql> =
                params_owned.iter().map(|v| v as &dyn ToSql).collect();

            // Execute the query
            let rows = {
                let mut stmt = tx.prepare(&query_owned)?;
                stmt.execute(&param_refs[..])?
            };
            tx.commit()?;

            Ok(rows)
        })
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Interact error: {}", e)))?
}
