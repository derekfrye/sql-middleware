/// `PostgreSQL` embedded database functionality
pub mod embedded;

/// `PostgreSQL` threading and concurrent tests
pub mod tests;

// Re-export the public API for backwards compatibility
pub use embedded::*;

// Module alias for backwards compatibility with the old testing_postgres name
pub mod testing_postgres {
    pub use super::*;
}
