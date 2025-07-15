//! Limbo/Turso module - provides Turso-specific database functionality
//!
//! This module is split into several sub-modules for better organization:
//! - config: Connection configuration and pool setup
//! - params: Parameter conversion between middleware and Turso types
//! - query: Result extraction and building
//! - executor: Database operation execution

pub mod config;
pub mod executor;
pub mod params;
pub mod query;

// Re-export the public API
pub use params::{LimboParams, LimboParamsExecute};
pub use query::build_result_set_from_rows as build_result_set;