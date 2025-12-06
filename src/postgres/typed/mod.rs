//! Experimental bb8-backed Postgres typestate API.
//! Provides `PgConnection<Idle>` / `PgConnection<InTx>` using an owned client
//! and explicit BEGIN/COMMIT/ROLLBACK.

mod core;
mod dml;
mod prepared;
mod select;
mod tx;

pub use core::{Idle, InTx, PgConnection, PgManager};
pub use dml::dml;
pub use select::select;
pub use tx::set_skip_drop_rollback_for_tests;
