//! Helper utilities for testing and development.

use crate::middleware::{CustomDbRow, RowValues};
use std::sync::Arc;

/// Create a test row with the given column names and values.
///
/// This is a utility function to help with testing by creating
/// CustomDbRow instances with the specified data.
pub fn create_test_row(column_names: Vec<String>, values: Vec<RowValues>) -> CustomDbRow {
    CustomDbRow::new(Arc::new(column_names), values)
}