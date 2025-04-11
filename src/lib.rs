/*!
 * SQL Middleware - A unified interface for SQL databases
 * 
 * This crate provides a middleware layer for SQL database access,
 * currently supporting SQLite, PostgreSQL, and SQL Server backends. The main goal is to 
 * provide a unified, async-compatible API that works across different database systems.
 * 
 * # Features
 * 
 * - Asynchronous database access with deadpool connection pooling
 * - Support for SQLite, PostgreSQL, and SQL Server backends
 * - Unified parameter conversion system
 * - Consistent result handling across database engines
 * - Transaction support
 * 
 * # Example
 * 
 * ```rust,no_run
 * use sql_middleware::prelude::*;
 * 
 * async fn sqlite_example() -> Result<(), SqlMiddlewareDbError> {
 *     // Create a SQLite connection pool
 *     let config = ConfigAndPool::new_sqlite("my_database.db".to_string()).await?;
 *     
 *     // Get a connection from the pool
 *     let pool = config.pool.get().await?;
 *     let mut conn = MiddlewarePool::get_connection(&pool).await?;
 *     
 *     // Execute a query with parameters
 *     let result = conn.execute_select(
 *         "SELECT * FROM users WHERE id = ?",
 *         &[RowValues::Int(1)]
 *     ).await?;
 *     
 *     // Process the results
 *     for row in result.results {
 *         println!("User: {}", row.get("name").unwrap().as_text().unwrap());
 *     }
 *     
 *     Ok(())
 * }
 * 
 * async fn postgres_example() -> Result<(), SqlMiddlewareDbError> {
 *     // Create a PostgreSQL connection pool
 *     let mut pg_config = deadpool_postgres::Config::new();
 *     pg_config.host = Some("localhost".to_string());
 *     pg_config.port = Some(5432);
 *     pg_config.dbname = Some("mydatabase".to_string());
 *     pg_config.user = Some("user".to_string());
 *     pg_config.password = Some("password".to_string());
 *     
 *     let config = ConfigAndPool::new_postgres(pg_config).await?;
 *     
 *     // Get a connection and execute a query
 *     let pool = config.pool.get().await?;
 *     let mut conn = MiddlewarePool::get_connection(&pool).await?;
 *     
 *     let result = conn.execute_select(
 *         "SELECT * FROM users WHERE id = $1",
 *         &[RowValues::Int(1)]
 *     ).await?;
 *     
 *     Ok(())
 * }
 * 
 * async fn sqlserver_example() -> Result<(), SqlMiddlewareDbError> {
 *     // Create an SQL Server connection pool
 *     let config = ConfigAndPool::new_mssql(
 *         "localhost".to_string(),
 *         "mydatabase".to_string(),
 *         "sa".to_string(),
 *         "strong_password".to_string(),
 *         Some(1433),
 *         None,
 *     ).await?;
 *     
 *     // Get a connection and execute a query
 *     let pool = config.pool.get().await?;
 *     let mut conn = MiddlewarePool::get_connection(&pool).await?;
 *     
 *     let result = conn.execute_select(
 *         "SELECT * FROM users WHERE id = @p1",
 *         &[RowValues::Int(1)]
 *     ).await?;
 *     
 *     Ok(())
 * }
 * ```
 */

 #![forbid(unsafe_code)]

// Test utilities module - only compiled with test-utils feature
#[cfg(feature = "test-utils")]
pub mod test_utils;

// Re-export everything that should be part of the public API
pub mod prelude {
    //! Convenient imports for common functionality.
    //! 
    //! This module re-exports the most commonly used types and functions
    //! to make it easier to get started with the library.
    
    pub use crate::middleware::{
        ConfigAndPool, 
        MiddlewarePool, 
        MiddlewarePoolConnection,
        AsyncDatabaseExecutor,
        ResultSet, 
        CustomDbRow,
        RowValues,
        SqlMiddlewareDbError,
        DatabaseType,
        ConversionMode,
        QueryAndParams,
        AnyConnWrapper,
    };
    
    pub use crate::convert_sql_params;
    pub use crate::postgres_build_result_set;
    pub use crate::sqlite_build_result_set;
    pub use crate::mssql_build_result_set;
    pub use crate::PostgresParams;
    pub use crate::SqliteParamsExecute;
    pub use crate::SqliteParamsQuery;
    pub use crate::MssqlParams;
    pub use crate::MssqlClient;
    pub use crate::MssqlManager;
    pub use crate::create_mssql_client;
}

// Core modules
pub mod middleware;

// Private database-specific modules
mod postgres;
mod sqlite;
mod mssql;

// Direct exports of frequently used types and functions for simplicity
pub use middleware::{
    ConfigAndPool, 
    MiddlewarePool, 
    MiddlewarePoolConnection,
    AsyncDatabaseExecutor,
    ResultSet, 
    CustomDbRow,
    RowValues,
    SqlMiddlewareDbError,
    DatabaseType,
    ConversionMode,
    QueryAndParams,
    AnyConnWrapper,
    ParamConverter,
};

pub use postgres::Params as PostgresParams;
pub use postgres::build_result_set as postgres_build_result_set;
pub use sqlite::build_result_set as sqlite_build_result_set;
pub use sqlite::SqliteParamsExecute;
pub use sqlite::SqliteParamsQuery;
pub use mssql::Params as MssqlParams;
pub use mssql::build_result_set as mssql_build_result_set;
pub use mssql::create_mssql_client;
pub use mssql::MssqlClient;
pub use mssql::MssqlManager;

// Module to help with testing - needed for existing tests
pub mod test_helpers {
    use std::sync::Arc;
    use crate::middleware::{CustomDbRow, RowValues};
    
    pub fn create_test_row(column_names: Vec<String>, values: Vec<RowValues>) -> CustomDbRow {
        CustomDbRow::new(Arc::new(column_names), values)
    }
}

/// Convert a slice of generic RowValues into database-specific parameters.
///
/// This function uses the ParamConverter trait to convert a set of parameters
/// into the format required by a specific database backend.
///
/// # Arguments
///
/// * `params` - The slice of RowValues to convert
/// * `mode` - Whether the parameters will be used for a query or execution
///
/// # Returns
///
/// The converted parameters, or an error if conversion fails
///
/// # Example
///
/// ```rust,no_run
/// use sql_middleware::prelude::*;
/// use sql_middleware::PostgresParams;
/// use sql_middleware::convert_sql_params;
///
/// fn convert_parameters<'a>(values: &'a [RowValues]) -> Result<PostgresParams<'a>, SqlMiddlewareDbError> {
///     let mode = ConversionMode::Query;
///     let postgres_params = convert_sql_params::<PostgresParams>(values, mode)?;
///     Ok(postgres_params)
/// }
/// ```
pub fn convert_sql_params<'a, T: ParamConverter<'a>>(
    params: &'a [RowValues],
    mode: ConversionMode,
) -> Result<T::Converted, SqlMiddlewareDbError> {
    // Check if the converter supports this mode
    if !T::supports_mode(mode) {
        return Err(SqlMiddlewareDbError::ParameterError(
            format!("Converter doesn't support mode: {:?}", mode)
        ));
    }
    T::convert_sql_params(params, mode)
}