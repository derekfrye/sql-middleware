// mssql.rs - SQL Server support via Tiberius
//! SQL Server (MSSQL) connection and query handling via Tiberius
//! 
//! This module provides the implementation of the middleware for Microsoft SQL Server.
//! It uses the Tiberius crate for connecting to and querying SQL Server databases.

use crate::middleware::{
    ConfigAndPool, ConversionMode, CustomDbRow, DatabaseType, MiddlewarePool, ParamConverter,
    ResultSet, RowValues, SqlMiddlewareDbError,
};
use async_trait::async_trait;
use chrono::{Datelike, NaiveDateTime, Timelike};
use deadpool::managed::{Manager, Object, Pool, RecycleError};
use futures_util::TryStreamExt;
use std::borrow::Cow;
use std::fmt;
use std::ops::DerefMut;
use tiberius::{AuthMethod, Client, ColumnData, IntoSql, ToSql};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

/// Type alias for SQL Server client
pub type MssqlClient = Client<Compat<TcpStream>>;

/// Manager for SQL Server connections (used with Deadpool)
#[derive(Clone)]
pub struct MssqlManager {
    config: tiberius::Config,
    server: String,
    port: u16,
}

impl fmt::Debug for MssqlManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MssqlManager")
            .field("server", &self.server)
            .field("port", &self.port)
            .finish()
    }
}

/// Parameter wrapper for SQL Server
pub enum SqlParam<'a> {
    Int(i64),
    Float(f64),
    Text(&'a str),
    Bool(bool), 
    Binary(&'a [u8]),
    None,
}

impl<'a> fmt::Debug for SqlParam<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqlParam::Int(i) => write!(f, "SqlParam::Int({})", i),
            SqlParam::Float(fl) => write!(f, "SqlParam::Float({})", fl),
            SqlParam::Text(s) => write!(f, "SqlParam::Text({})", s),
            SqlParam::Bool(b) => write!(f, "SqlParam::Bool({})", b),
            SqlParam::Binary(_) => write!(f, "SqlParam::Binary(...)"),
            SqlParam::None => write!(f, "SqlParam::None"),
        }
    }
}

impl<'a> IntoSql<'a> for SqlParam<'a> {
    fn into_sql(self) -> ColumnData<'a> {
        match self {
            SqlParam::Int(i) => ColumnData::I64(Some(i)),
            SqlParam::Float(f) => ColumnData::F64(Some(f)),
            SqlParam::Text(s) => ColumnData::String(Some(Cow::from(s))),
            SqlParam::Bool(b) => ColumnData::Bit(Some(b)),
            SqlParam::Binary(b) => ColumnData::Binary(Some(Cow::from(b))),
            SqlParam::None => ColumnData::String(None),
        }
    }
}

// Implementation of the deadpool Manager trait for MssqlManager
#[async_trait]
impl Manager for MssqlManager {
    type Type = MssqlClient;
    type Error = tiberius::error::Error;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let config = self.config.clone();
        
        // Connect to SQL Server
        let addr = format!("{}:{}", self.server, self.port);
        let tcp = TcpStream::connect(addr).await
            .map_err(|e| tiberius::error::Error::Io {
                kind: e.kind(),
                message: format!("TCP connection error: {}", e),
            })?;
        
        let tcp = tcp.compat_write();
        Client::connect(config, tcp).await
    }

    async fn recycle(
        &self,
        client: &mut Self::Type,
        _metrics: &deadpool::managed::Metrics,
    ) -> Result<(), RecycleError<Self::Error>> {
        // Check if connection is still usable by running a simple query
        let query = tiberius::Query::new("SELECT 1");
        match query.query(client).await {
            Ok(_) => Ok(()),
            Err(e) => Err(RecycleError::Backend(e)),
        }
    }
}

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with SQL Server (MSSQL)
    pub async fn new_mssql(
        server: String,
        database: String,
        user: String,
        password: String,
        port: Option<u16>,
        _instance_name: Option<String>, // For future named instance support
    ) -> Result<Self, SqlMiddlewareDbError> {
        // Configure SQL Server connection
        let mut config = tiberius::Config::new();
        config.host(&server);
        config.database(&database);
        config.authentication(AuthMethod::sql_server(&user, &password));
        
        let port_val = port.unwrap_or(1433);
        config.port(port_val);
        
        // Create manager for SQL Server connections
        let manager = MssqlManager {
            config,
            server,
            port: port_val,
        };
        
        // Create deadpool connection pool
        let pool = Pool::builder(manager)
            .max_size(20)
            .build()
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
        let references: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

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

/// Convert a RowValues to a SqlParam that can be bound to a Tiberius query
fn row_value_to_sql_param<'a>(row_value: &'a RowValues) -> SqlParam<'a> {
    match row_value {
        RowValues::Int(i) => SqlParam::Int(*i),
        RowValues::Float(f) => SqlParam::Float(*f),
        RowValues::Text(s) => SqlParam::Text(s),
        RowValues::Bool(b) => SqlParam::Bool(*b),
        RowValues::Timestamp(dt) => {
            // For timestamps, convert to ISO-8601 string
            SqlParam::Text(&format!("{}", dt))
        },
        RowValues::Null => SqlParam::None,
        RowValues::JSON(jsval) => SqlParam::Text(jsval.to_string().as_str()),
        RowValues::Blob(bytes) => SqlParam::Binary(bytes),
    }
}

/// ToSql for RowValues for passing parameters
impl ToSql for RowValues {
    fn to_sql(&self) -> tiberius::ColumnData<'_> {
        match self {
            RowValues::Int(i) => ColumnData::I64(Some(*i)),
            RowValues::Float(f) => ColumnData::F64(Some(*f)),
            RowValues::Text(s) => ColumnData::String(Some(Cow::from(s.as_str()))),
            RowValues::Bool(b) => ColumnData::Bit(Some(*b)),
            RowValues::Timestamp(dt) => {
                // Convert to string for simplicity
                ColumnData::String(Some(Cow::from(dt.to_string())))
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
    // Create and prepare the query
    let mut query_builder = tiberius::Query::new(query);
    
    // Add parameters
    for param in params {
        let sql_param = row_value_to_sql_param(param);
        query_builder.bind(sql_param);
    }

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

    // Process the stream
    let mut rows_stream = stream.into_row_stream();
    while let Some(row_result) = rows_stream.try_next().await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server row fetch error: {}", e)))?
    {
        let mut row_values = Vec::with_capacity(column_names_rc.len());

        for i in 0..column_names_rc.len() {
            // Extract values from the row
            if let Some(value) = extract_value(&row_result, i)? {
                row_values.push(value);
            } else {
                row_values.push(RowValues::Null);
            }
        }

        result_set.add_row(CustomDbRow::new(column_names_rc.clone(), row_values));
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

/// Execute a DML query (INSERT, UPDATE, DELETE) with parameters
pub async fn execute_dml(
    mssql_client: &mut Object<MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    // Get a client from the object
    let client = mssql_client.deref_mut();

    // Prepare the query with parameters
    let mut query_builder = tiberius::Query::new(query);
    
    // Add parameters
    for param in params {
        let sql_param = row_value_to_sql_param(param);
        query_builder.bind(sql_param);
    }

    // Execute the query
    let exec_result = query_builder.execute(client).await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("SQL Server DML execution error: {}", e)))?;

    // Get rows affected
    let rows_affected: u64 = exec_result.rows_affected().into_iter().sum();

    Ok(rows_affected as usize)
}

// Helper function to create a new MSSQL connection
pub async fn create_mssql_client(
    server: &str,
    database: &str,
    user: &str,
    password: &str,
    port: Option<u16>,
    _instance_name: Option<&str>, // Not fully supported yet
) -> Result<MssqlClient, SqlMiddlewareDbError> {
    let mut config = tiberius::Config::new();
    
    config.host(server);
    config.database(database);
    config.authentication(AuthMethod::sql_server(user, password));
    
    let port_val = port.unwrap_or(1433);
    config.port(port_val);

    // Create TCP stream 
    let tcp = TcpStream::connect(format!("{}:{}", server, port_val)).await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("TCP connection error: {}", e)))?;
    
    // Make compatible with Tiberius
    let tcp = tcp.compat_write();
    
    // Connect with Tiberius
    Client::connect(config, tcp).await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("SQL Server connection error: {}", e)))
}