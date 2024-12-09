//! MyLibrary: A library for managing databases with Postgres and SQLite.
//!
//! ## Features
//! - Connection pooling for Postgres and SQLite.
//! - Query execution with abstracted row handling.
//! - Models for database tables, rows, and states.
pub mod model;
pub mod db{
    pub mod db;
    pub mod convenience_items;
}

