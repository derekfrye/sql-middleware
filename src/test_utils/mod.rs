use std::sync::LazyLock;
use tokio::runtime::Runtime;

/// Shared tokio runtime for test utilities to avoid creating multiple runtimes
pub(crate) static SHARED_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("Failed to create tokio runtime for test utilities"));

/// Test utilities for `PostgreSQL` testing and benchmarking
pub mod postgres;

// Re-export the most commonly used items for backwards compatibility
pub use postgres::*;

// Module alias for backwards compatibility with the old testing_postgres name
pub mod testing_postgres {
    pub use super::postgres::*;
}
