//! Typestate MSSQL connection wrappers.
//!
//! Provides `MssqlTypedConnection<Idle>` / `MssqlTypedConnection<InTx>` using an owned
//! pooled Tiberius client.

mod core;
mod dml;
mod select;
mod tx;

pub use core::{Idle, InTx, MssqlManager, MssqlTypedConnection};
pub use dml::dml;
pub use select::select;
pub use tx::set_skip_drop_rollback_for_tests;
