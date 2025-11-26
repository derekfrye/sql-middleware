//! SQLite backend glue.
//!
//! Blocking `rusqlite` work runs on a dedicated worker thread behind each pooled connection, so
//! async tasks do not stall the runtime. Key helpers:
//! - `with_sqlite_connection` to run batched work against the raw `rusqlite::Connection`
//! - `prepare_sqlite_statement` for cached prepared statements on the worker
//! - `begin_transaction`/`Tx` for explicit transactions on the worker connection
//!
//! Submodules:
//! - `config`: connection configuration and pool setup
//! - `params`: parameter conversion between middleware and SQLite types
//! - `query`: result extraction and building
//! - `executor`: database operation execution

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
pub use prepared::SqlitePreparedStatement;
#[allow(unused_imports)]
pub use query::build_result_set;
#[allow(unused_imports)]
pub use transaction::{Prepared, Tx, begin_transaction};
#[allow(unused_imports)]
pub use worker::SqliteConnection;
