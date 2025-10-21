// Turso module - provides turso-specific database functionality
//
// This module mirrors the libsql module structure for consistency:
// - config: Connection configuration and pool setup
// - params: Parameter conversion between middleware and turso types
// - query: Result extraction and building
// - executor: Database operation execution

pub mod config;
pub mod executor;
pub mod params;
pub mod prepared;
pub mod query;
pub mod transaction;

// Re-export the public API for convenience
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::Params;
pub use prepared::TursoPreparedStatement;
pub use query::build_result_set;
pub use transaction::{Prepared, Tx, begin_transaction, with_transaction};
