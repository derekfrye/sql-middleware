//! Turso backend glue (SQLite-compatible, in-process).
//!
//! Mirrors the LibSQL/SQLite module layout:
//! - `config`: connection configuration and pool setup
//! - `params`: parameter conversion between middleware and Turso types
//! - `query`: result extraction and building
//! - `executor`: database operation execution

pub mod config;
pub mod executor;
pub mod params;
pub mod prepared;
pub mod query;
pub mod transaction;

// Re-export the public API for convenience
pub use config::{TursoOptions, TursoOptionsBuilder};
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::Params;
pub use prepared::TursoNonTxPreparedStatement;
pub use query::build_result_set;
pub use transaction::{Prepared, Tx, begin_transaction};
