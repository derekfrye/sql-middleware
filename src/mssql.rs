// mssql.rs - SQL Server support via Tiberius and deadpool-tiberius
//! SQL Server (MSSQL) connection and query handling via Tiberius
//! 
//! This module provides the implementation of the middleware for Microsoft SQL Server.
//! It uses the Tiberius crate for connecting to and querying SQL Server databases,
//! with connection pooling provided by deadpool.

use crate::middleware::{
    ConfigAndPool, ConversionMode, DatabaseType, MiddlewarePool, ParamConverter,
    ResultSet, RowValues, SqlMiddlewareDbError,
};
use chrono::NaiveDateTime;
use deadpool::managed::Object;
use deadpool_tiberius::Manager as TiberiusManager;
use futures_util::TryStreamExt;
use std::borrow::Cow;
use std::ops::DerefMut;
use tiberius::{Client, ColumnData, ToSql};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

/// Type alias for SQL Server client
pub type MssqlClient = Client<Compat<TcpStream>>;

/// Type alias for SQL Server Manager from deadpool-tiberius
pub type MssqlManager = TiberiusManager;

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with SQL Server (MSSQL)
    pub async fn new_mssql(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        instance_name: Option<String>, // For named instance support
    ) -> Result<Self, SqlMiddlewareDbError> {
        // Create deadpool-tiberius manager and configure it
        let mut manager = deadpool_tiberius::Manager::new()
            .host(&server)
            .port(port.unwrap_or(1433))
            .database(&database)
            .basic_authentication(&user, &password)
            .trust_cert();
            
        // Add instance name if provided
        if let Some(instance) = &instance_name {
            manager = manager.instance_name(instance);
        }
        
        // Create pool
        let pool = manager
            .max_size(20)
            .create_pool()
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQL Server pool: {}", e)))?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Mssql(pool),
            db_type: DatabaseType::Mssql,
        })
    }
}

/// Container for SQL Server parameters with lifetime tracking
pub struct Params<'a> {
    references: Vec<&'a (dyn ToSql + Sync)>,
}

impl<'a> Params<'a> {
    /// Convert from a slice of RowValues to SQL Server parameters
    pub fn convert(params: &'a [RowValues]) -> Result<Params<'a>, SqlMiddlewareDbError> {
        // Pre-allocate space for performance
        let mut references = Vec::with_capacity(params.len());
        
        // Avoid iterator.collect() allocation overhead
        for p in params {
            references.push(p as &(dyn ToSql + Sync));
        }

        Ok(Params { references })
    }

    /// Get a reference to the underlying parameter array
    pub fn as_refs(&self) -> &[&(dyn ToSql + Sync)] {
        &self.references
    }
}

impl<'a> ParamConverter<'a> for Params<'a> {
    type Converted = Params<'a>;

    fn convert_sql_params(
        params: &'a [RowValues],
        _mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        Self::convert(params)
    }
    
    // SQL Server params support both query and execution modes
    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}

// Direct conversion methods are implemented in the bind_query_params function
// and ToSql implementation for RowValues

/// ToSql for RowValues for passing parameters
impl ToSql for RowValues {
    fn to_sql(&self) -> tiberius::ColumnData<'_> {
        match self {
            RowValues::Int(i) => ColumnData::I64(Some(*i)),
            RowValues::Float(f) => ColumnData::F64(Some(*f)),
            RowValues::Text(s) => ColumnData::String(Some(Cow::from(s.as_str()))),
            RowValues::Bool(b) => ColumnData::Bit(Some(*b)),
            RowValues::Timestamp(dt) => {
                // Use thread_local storage for efficient timestamp formatting
                thread_local! {
                    static BUF: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(32));
                }
                
                // Format the timestamp efficiently
                let formatted = BUF.with(|buf| {
                    let mut s = buf.borrow_mut();
                    s.clear();
                    use std::fmt::Write;
                    // ISO-8601 format
                    write!(s, "{}", dt.format("%Y-%m-%dT%H:%M:%S%.f")).unwrap();
                    ColumnData::String(Some(Cow::from(s.clone())))
                });
                
                formatted
            },
            RowValues::Null => ColumnData::String(None),
            RowValues::JSON(jsval) => ColumnData::String(Some(Cow::from(jsval.to_string()))),
            RowValues::Blob(bytes) => ColumnData::Binary(Some(Cow::from(bytes.as_slice()))),
        }
    }
}

/// Build a result set from a SQL Server query execution
pub async fn build_result_set(
    client: &mut MssqlClient,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Use the shared function to prepare and bind the query
    let query_builder = bind_query_params(query, params);

    // Execute the query
    let mut stream = query_builder.query(client).await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server query error: {}", e)))?;

    // Get column information
    let columns_opt = stream.columns().await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server column fetch error: {}", e)))?;
    
    let columns = columns_opt.ok_or_else(|| 
        SqlMiddlewareDbError::ExecutionError("No columns returned from query".to_string()))?;
    
    let column_names: Vec<String> = columns
        .iter()
        .map(|col| col.name().to_string())
        .collect();

    // Preallocate capacity if we can estimate the number of rows
    let mut result_set = ResultSet::with_capacity(10);
    // Store column names once in the result set
    let column_names_rc = std::sync::Arc::new(column_names);
    result_set.set_column_names(column_names_rc);

    // Process the stream
    let mut rows_stream = stream.into_row_stream();
    while let Some(row_result) = rows_stream.try_next().await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server row fetch error: {}", e)))?
    {
        let col_count = result_set.get_column_names()
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("No column names available".to_string()))?
            .len();
            
        let mut row_values = Vec::with_capacity(col_count);
            
        for i in 0..col_count {
            // Extract values from the row
            if let Some(value) = extract_value(&row_result, i)? {
                row_values.push(value);
            } else {
                row_values.push(RowValues::Null);
            }
        }

        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}

/// Extract a value from a row at a specific index
fn extract_value(
    row: &tiberius::Row,
    idx: usize,
) -> Result<Option<RowValues>, SqlMiddlewareDbError> {
    // Since Tiberius Row API is a bit complex and varies by version,
    // we'll use a simple approach by trying different value types
    
    // Try integer
    if let Ok(Some(val)) = row.try_get::<i32, _>(idx) {
        return Ok(Some(RowValues::Int(val as i64)));
    }
    
    if let Ok(Some(val)) = row.try_get::<i64, _>(idx) {
        return Ok(Some(RowValues::Int(val)));
    }
    
    // Try floating point
    if let Ok(Some(val)) = row.try_get::<f32, _>(idx) {
        return Ok(Some(RowValues::Float(val as f64)));
    }
    
    if let Ok(Some(val)) = row.try_get::<f64, _>(idx) {
        return Ok(Some(RowValues::Float(val)));
    }
    
    // Try boolean
    if let Ok(Some(val)) = row.try_get::<bool, _>(idx) {
        return Ok(Some(RowValues::Bool(val)));
    }
    
    // Try string (most values can be represented as strings)
    if let Ok(Some(val)) = row.try_get::<&str, _>(idx) {
        // If it looks like a date/time, try to parse it
        if val.contains('-') && (val.contains(':') || val.contains(' ')) {
            if let Ok(dt) = NaiveDateTime::parse_from_str(val, "%Y-%m-%d %H:%M:%S%.f") {
                return Ok(Some(RowValues::Timestamp(dt)));
            } else if let Ok(dt) = NaiveDateTime::parse_from_str(val, "%Y-%m-%d %H:%M:%S") {
                return Ok(Some(RowValues::Timestamp(dt)));
            }
        }
        
        // Otherwise, just return as text
        return Ok(Some(RowValues::Text(val.to_string())));
    }
    
    // Try bytes (binary data)
    if let Ok(Some(val)) = row.try_get::<&[u8], _>(idx) {
        return Ok(Some(RowValues::Blob(val.to_vec())));
    }
    
    // Check if the value is NULL
    if let Ok(None) = row.try_get::<&str, _>(idx) {
        return Ok(None);
    }
    
    // If none of the above worked, return NULL
    Ok(None)
}

/// Execute a batch of SQL statements for SQL Server
pub async fn execute_batch(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    // Get a client from the object
    let client = mssql_client.deref_mut();

    // Execute the batch of queries
    let query_builder = tiberius::Query::new(query);
    query_builder.execute(client).await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server batch execution error: {}", e)))?;

    Ok(())
}

/// Execute a SELECT query with parameters
pub async fn execute_select(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Get a client from the object
    let client = mssql_client.deref_mut();
    
    // Use the build_result_set function to handle parameters and execution
    build_result_set(client, query, params).await
}

/// Bind parameters directly to the query for SQL Server
/// Return a query builder with parameters already bound
fn bind_query_params<'a>(
    query: &'a str,
    params: &[RowValues],
) -> tiberius::Query<'a> {
    // Create the query builder
    let mut query_builder = tiberius::Query::new(query);
    
    // Bind parameters directly - not using OwnedParam as intermediary
    // since tiberius Query will own the data
    for param in params {
        match param {
            RowValues::Int(i) => query_builder.bind(*i),
            RowValues::Float(f) => query_builder.bind(*f),
            RowValues::Text(s) => query_builder.bind(s.clone()),
            RowValues::Bool(b) => query_builder.bind(*b),
            RowValues::Timestamp(dt) => {
                // Format timestamps efficiently
                let formatted = dt.format("%Y-%m-%dT%H:%M:%S%.f").to_string();
                query_builder.bind(formatted);
            },
            RowValues::Null => query_builder.bind(Option::<String>::None),
            RowValues::JSON(jsval) => query_builder.bind(jsval.to_string()),
            RowValues::Blob(bytes) => query_builder.bind(bytes.clone()),
        }
    }
    
    query_builder
}

/// Execute a DML query (INSERT, UPDATE, DELETE) with parameters
pub async fn execute_dml(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    // Get a client from the object
    let client = mssql_client.deref_mut();

    // Prepare and bind the query
    let query_builder = bind_query_params(query, params);

    // Execute the query
    let exec_result = query_builder.execute(client).await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server DML execution error: {}", e)))?;

    // Get rows affected
    let rows_affected: u64 = exec_result.rows_affected().iter().sum();

    Ok(rows_affected as usize)
}

// Helper function to create a new MSSQL connection
pub async fn create_mssql_client(
    server: &str,
    database: &str,
    user: &str,
    password: &str,
    port: Option<u16>,
    instance_name: Option<&str>,
) -> Result<MssqlClient, SqlMiddlewareDbError> {
    let mut config = tiberius::Config::new();
    
    config.host(server);
    config.database(database);
    config.authentication(tiberius::AuthMethod::sql_server(user, password));
    
    let port_val = port.unwrap_or(1433);
    config.port(port_val);
    
    if let Some(instance) = instance_name {
        config.instance_name(instance);
    }
    
    config.trust_cert(); // For testing/development

    // Create TCP stream using proper socket address
    use std::net::ToSocketAddrs;
    
    // Try to resolve the socket address
    let addr_iter = (server, port_val).to_socket_addrs()
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to resolve server address: {}", e)))?;
    
    // Find the first valid address
    let server_addr = addr_iter.into_iter()
        .next()
        .ok_or_else(|| SqlMiddlewareDbError::ConnectionError(format!("No valid address found for {}", server)))?;
    
    // Connect to the resolved socket address
    let tcp = TcpStream::connect(server_addr).await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("TCP connection error: {}", e)))?;
    
    // Make compatible with Tiberius
    let tcp = tcp.compat_write();
    
    // Connect with Tiberius
    Client::connect(config, tcp).await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("SQL Server connection error: {}", e)))
}