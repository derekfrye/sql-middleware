// SQLite module - provides SQLite-specific database functionality
//
// This module is split into several sub-modules for better organization:
// - config: Connection configuration and pool setup
// - params: Parameter conversion between middleware and SQLite types
// - query: Result extraction and building
// - executor: Database operation execution

pub mod config;
pub mod executor;
pub mod params;
pub mod query;
pub mod worker;

// Re-export the public API
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::{SqliteParamsExecute, SqliteParamsQuery};
pub use query::build_result_set;
pub use worker::SqliteConnection;
