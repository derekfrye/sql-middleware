//! Bb8-backed `SQLite` typestate API.
//! Provides `SqliteTypedConnection<Idle>` / `SqliteTypedConnection<InTx>` using an owned
//! pooled `SQLite` connection with explicit BEGIN/COMMIT/ROLLBACK.

mod core;
mod dml;
mod prepared;
mod select;
mod tx;

pub use core::{Idle, InTx, SqliteTypedConnection};
pub use dml::dml;
pub use select::select;
pub use tx::set_skip_drop_rollback_for_tests;
