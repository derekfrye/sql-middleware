//! `PostgreSQL` backend glue.
//!
//! Submodules mirror the SQLite/Turso/LibSQL structure for consistency:
//! - `config`: connection configuration and pool setup
//! - `params`: parameter conversion between middleware and `PostgreSQL` types
//! - `query`: result extraction and building
//! - `executor`: database operation execution

pub mod config;
pub mod executor;
pub mod params;
pub mod query;
pub mod transaction;
pub mod typed;

// Re-export the public API
pub use config::{PgConfig, PostgresOptions, PostgresOptionsBuilder};
pub use executor::{execute_batch, execute_dml, execute_select};
pub use params::Params;
pub use query::build_result_set;
pub use transaction::{Prepared, Tx, begin_transaction};
pub use typed::{
    Idle as TypedIdle, InTx as TypedInTx, PgConnection as TypedPgConnection, PgManager,
};
