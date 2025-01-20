//! MyLibrary: A library for managing databases with Postgres and SQLite.
//!
//! ## Features
//! - Connection pooling for Postgres and SQLite.
//! - Query execution with abstracted row handling.
//! - Models for database tables, rows, and states.
pub mod model;
pub mod convenience_items;
pub mod db;
pub use sqlx::FromRow;
pub mod db2;
pub mod middleware;
mod sqlite;
mod postgres;
pub mod db_model;

pub use sqlite::convert_params;
pub use sqlite::build_result_set;