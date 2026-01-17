//! Bb8-backed Turso typestate API.
//! Provides `TursoConnection<Idle>` / `TursoConnection<InTx>` using an owned Turso connection
//! with explicit BEGIN/COMMIT/ROLLBACK.

mod core;
mod dml;
mod prepared;
mod select;
mod tx;

pub use core::{Idle, InTx, TursoConnection, TursoManager};
pub use dml::dml;
pub use select::select;
pub use tx::set_skip_drop_rollback_for_tests;
