//! SQL Server backend glue (mirrors the layout used by the other backends).
//!
//! Submodules:
//! - `config`: connection configuration and pool setup (builder pattern)
//! - `params`: parameter conversion between middleware and SQL Server types
//! - `query`: result extraction, building, and query binding
//! - `executor`: database operation execution
//! - `client`: raw client creation utilities

pub mod client;
pub mod config;
pub mod executor;
pub mod params;
pub mod prepared;
pub mod query;
pub mod transaction;

// Re-export the public API
pub use client::create_mssql_client;
pub use config::{MssqlClient, MssqlOptions, MssqlOptionsBuilder};
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::Params;
pub use prepared::MssqlNonTxPreparedStatement;
pub use query::build_result_set;
pub use transaction::{Prepared, Tx, begin_transaction};
