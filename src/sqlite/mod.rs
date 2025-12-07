//! `SQLite` backend glue backed by a bb8 pool of `rusqlite` connections.
//!
//! Blocking `rusqlite` work runs on a dedicated worker thread per pooled connection,
//! keeping the async runtime responsive while avoiding deadpool-sqlite.
//!
//! Submodules:
//! - `config`: connection configuration and pool setup
//! - `params`: parameter conversion between middleware and `SQLite` types
//! - `query`: result extraction and building
//! - `executor`: database operation execution
//! - `transaction`: explicit transaction support
//! - `prepared`: prepared statement helpers

pub mod config;
pub mod connection;
pub mod executor;
pub mod params;
pub mod prepared;
pub mod query;
pub mod transaction;
pub mod typed;

// Re-export the public API
#[allow(unused_imports)]
pub use config::{SqliteOptions, SqliteOptionsBuilder};
#[allow(unused_imports)]
pub use connection::{SqliteConnection, apply_wal_pragmas};
#[allow(unused_imports)]
pub use executor::{execute_batch, execute_dml, execute_select};
#[allow(unused_imports)]
pub use params::Params;
pub use prepared::SqlitePreparedStatement;
#[allow(unused_imports)]
pub use query::build_result_set;
#[allow(unused_imports)]
pub use transaction::{Prepared, Tx, begin_transaction};
#[allow(unused_imports)]
pub use typed::{Idle, InTx, SqliteTypedConnection};
