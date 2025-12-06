//! Backend-agnostic typestate traits and enums for typed connections.
//!
//! This module provides traits for typed database connections with compile-time
//! transaction state tracking, plus backend-neutral `AnyIdle`/`AnyTx` wrappers.

mod any;
#[cfg(feature = "postgres")]
mod impl_postgres;
#[cfg(feature = "sqlite")]
mod impl_sqlite;
#[cfg(feature = "turso")]
mod impl_turso;
mod traits;

// Re-export everything for public API
pub use any::{AnyIdle, AnyTx};
pub use traits::{BeginTx, Queryable, TxConn, TypedConnOps};
