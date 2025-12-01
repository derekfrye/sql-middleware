//! SQL Server backend glue.
//!
//! Submodules:
//! - `config`: connection configuration and pool setup
//! - `params`: parameter conversion between middleware and SQL Server types
//! - `query`: result extraction, building, and query binding
//! - `executor`: database operation execution
//! - `client`: raw client creation utilities

pub mod client;
pub mod config;
pub mod executor;
pub mod params;
pub mod query;

// Re-export the public API
#[allow(unused_imports)]
pub use client::create_mssql_client;
#[allow(unused_imports)]
pub use config::MssqlClient;
#[allow(unused_imports)]
pub use config::{MssqlOptions, MssqlOptionsBuilder};
#[allow(unused_imports)]
pub use executor::{execute_batch, execute_dml, execute_select};
#[allow(unused_imports)]
pub use params::Params;
#[allow(unused_imports)]
pub use query::build_result_set;
