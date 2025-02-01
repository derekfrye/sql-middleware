//! MyLibrary: A library for managing databases with Postgres and SQLite.
//!
//! ## Features
//! - Connection pooling for Postgres and SQLite.
//! - Query execution with abstracted row handling.
//! - Models for database tables, rows, and states.
// pub mod convenience_items;
// pub mod db;
// pub mod model;
// pub use sqlx::FromRow;
// pub mod db_model;
pub mod middleware;
mod postgres;
mod sqlite;

use deadpool_sqlite::rusqlite;
pub use middleware::SqlMiddlewareDbError;
pub use postgres::build_result_set as postgres_build_result_set;
pub use postgres::Params as PostgresParams;
pub use rusqlite::params_from_iter as sqlite_params_from_iter;
pub use deadpool_sqlite::rusqlite::params as sqlite_params;
pub use sqlite::build_result_set as sqlite_build_result_set;
pub use sqlite::convert_params as sqlite_convert_params;
pub use sqlite::convert_params_for_execute as sqlite_convert_params_for_execute;