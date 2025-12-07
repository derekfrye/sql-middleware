//! `LibSQL` backend glue (local or remote).
//!
//! Mirrors the SQLite/Turso module layout:
//! - `config`: connection configuration and pool setup
//! - `params`: parameter conversion between middleware and `LibSQL` types
//! - `query`: result extraction and building
//! - `executor`: database operation execution

pub mod config;
pub mod executor;
pub mod params;
pub mod query;
pub mod transaction;

// Re-export the public API
pub use config::{
    LibsqlOptions, LibsqlOptionsBuilder, LibsqlRemoteOptions, LibsqlRemoteOptionsBuilder,
};
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::Params;
pub use query::build_result_set;
pub use transaction::{Prepared, Tx, begin_transaction};
