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
pub mod prepared;
pub mod query;
pub mod transaction;
pub mod worker;

// Re-export the public API
#[allow(unused_imports)]
pub use executor::{execute_batch, execute_dml, execute_select};
#[allow(unused_imports)]
pub use params::Params;
#[allow(unused_imports)]
pub use query::build_result_set;
#[allow(unused_imports)]
pub use transaction::{Prepared, Tx, begin_transaction};
#[allow(unused_imports)]
pub use worker::SqliteConnection;
#[allow(unused_imports)]
pub use prepared::SqlitePreparedStatement;
