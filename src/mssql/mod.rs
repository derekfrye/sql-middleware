// MSSQL module - provides SQL Server-specific database functionality
//
// This module is split into several sub-modules for better organization:
// - config: Connection configuration and pool setup
// - params: Parameter conversion between middleware and SQL Server types
// - query: Result extraction, building, and query binding
// - executor: Database operation execution
// - client: Raw client creation utilities

pub mod client;
pub mod config;
pub mod executor;
pub mod params;
pub mod query;

// Re-export the public API
pub use client::create_mssql_client;
pub use config::MssqlClient;
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::Params;
pub use query::build_result_set;
