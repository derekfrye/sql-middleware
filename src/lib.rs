/*!
 * SQL Middleware - A unified interface for SQL databases
 * 
 * This crate provides a middleware layer for SQL database access,
 * currently supporting SQLite and PostgreSQL backends. The main goal is to 
 * provide a unified, async-compatible API that works across different database systems.
 * 
 * # Features
 * 
 * - Asynchronous database access with deadpool connection pooling
 * - Support for SQLite and PostgreSQL backends
 * - Unified parameter conversion system
 * - Consistent result handling across database engines
 * - Transaction support
 * 
 * # Example
 * 
 * ```rust,no_run
 * use sql_middleware::prelude::*;
 * 
 * async fn example() -> Result<(), SqlMiddlewareDbError> {
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
 * ```
 */

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
    };
    
    pub use crate::convert_sql_params;
    pub use crate::params::{PostgresParams, SqliteParamsExecute, SqliteParamsQuery};
}

// Core modules
pub mod middleware;

// Private database-specific modules
mod postgres;
mod sqlite;

// Public parameter conversion module
pub mod params {
    //! Parameter conversion types for different database backends.
    //! 
    //! This module provides the types needed to convert generic parameter
    //! values into database-specific formats.
    
    pub use crate::postgres::Params as PostgresParams;
    pub use crate::sqlite::SqliteParamsExecute;
    pub use crate::sqlite::SqliteParamsQuery;
}

// Public database-specific functionality
pub mod db {
    //! Database-specific functionality.
    //! 
    //! This module exposes certain database-specific functions that
    //! may be needed for advanced use cases.
    
    pub use crate::postgres::build_result_set as postgres_build_result_set;
    pub use crate::sqlite::build_result_set as sqlite_build_result_set;
}

// Legacy exports for backward compatibility with existing tests and code
// These will be deprecated and eventually removed
#[doc(hidden)]
pub use middleware::SqlMiddlewareDbError;
#[doc(hidden)]
pub use postgres::build_result_set as postgres_build_result_set;
#[doc(hidden)]
pub use sqlite::build_result_set as sqlite_build_result_set;
#[doc(hidden)]
pub use params::PostgresParams;
#[doc(hidden)]
pub use params::SqliteParamsExecute;
#[doc(hidden)]
pub use params::SqliteParamsQuery;

// Module to help with testing
/// Utilities for testing the sql-middleware crate
pub mod test_helpers {
    //! Utilities for testing the sql-middleware crate.
    //!
    //! This module provides helper functions and types for writing tests
    //! against the sql-middleware API.
    
    use std::sync::Arc;
    use crate::middleware::{CustomDbRow, RowValues};
    
    /// Create a CustomDbRow for testing purposes
    pub fn create_test_row(column_names: Vec<String>, values: Vec<RowValues>) -> CustomDbRow {
        CustomDbRow::new(Arc::new(column_names), values)
    }
}

// Re-export important traits
use middleware::ConversionMode;
use middleware::ParamConverter;
use middleware::RowValues;

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
/// use sql_middleware::params::PostgresParams;
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