//! Bb8-backed Postgres typestate API.
//! Provides `PgConnection<Idle>` / `PgConnection<InTx>` using an owned client
//! and explicit BEGIN/COMMIT/ROLLBACK.

mod core;
mod dml;
mod prepared;
mod select;
mod tx;

pub use core::{Idle, InTx, PgConnection, PgManager};
pub use dml::{dml, dml_prepared};
pub use select::{select, select_prepared};
pub use tx::set_skip_drop_rollback_for_tests;
