// MSSQL module - provides SQL Server-specific database functionality
//
// This module is split into several sub-modules for better organization:
// - config: Connection configuration and pool setup
// - params: Parameter conversion between middleware and SQL Server types
// - query: Result extraction, building, and query binding
// - executor: Database operation execution
// - client: Raw client creation utilities

pub mod config;
pub mod params;
pub mod query;
pub mod executor;
pub mod client;

// Re-export the public API
pub use config::MssqlClient;
pub use params::Params;
pub use query::build_result_set;
pub use executor::{execute_batch, execute_select, execute_dml};
pub use client::create_mssql_client;