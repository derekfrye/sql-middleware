use crate::types::RowValues;

/// A row from a database query result
///
/// This struct represents a single row from a database query result,
/// with access to both the column names and the values.
#[derive(Debug, Clone)]
pub struct CustomDbRow {
    /// The column names for this row (shared across all rows in a result set)
    pub column_names: std::sync::Arc<Vec<String>>,
    /// The values for this row
    pub rows: Vec<RowValues>,
    // Internal cache for faster column lookups (to avoid repeated string comparisons)
    #[doc(hidden)]
    pub(crate) column_index_cache: std::sync::Arc<std::collections::HashMap<String, usize>>,
}

impl CustomDbRow {
    /// Create a new database row
    ///
    /// # Arguments
    ///
    /// * `column_names` - The column names
    /// * `rows` - The values for this row
    ///
    /// # Returns
    ///
    /// A new `CustomDbRow` instance
    #[must_use]
    pub fn new(column_names: std::sync::Arc<Vec<String>>, rows: Vec<RowValues>) -> Self {
        // Build a cache of column name to index for faster lookups
        let cache = std::sync::Arc::new(
            column_names
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), i))
                .collect::<std::collections::HashMap<_, _>>(),
        );

        Self {
            column_names,
            rows,
            column_index_cache: cache,
        }
    }

    /// Get the index of a column by name
    ///
    /// # Arguments
    ///
    /// * `column_name` - The name of the column
    ///
    /// # Returns
    ///
    /// The index of the column, or None if not found
    #[must_use]
    pub fn get_column_index(&self, column_name: &str) -> Option<usize> {
        // First check the cache
        if let Some(&idx) = self.column_index_cache.get(column_name) {
            return Some(idx);
        }

        // Fall back to linear search
        self.column_names.iter().position(|col| col == column_name)
    }

    /// Get a value from the row by column name
    ///
    /// # Arguments
    ///
    /// * `column_name` - The name of the column
    ///
    /// # Returns
    ///
    /// The value at the column, or None if the column wasn't found
    #[must_use]
    pub fn get(&self, column_name: &str) -> Option<&RowValues> {
        let index_opt = self.get_column_index(column_name);
        if let Some(idx) = index_opt {
            self.rows.get(idx)
        } else {
            None
        }
    }

    /// Get a value from the row by column index
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the column
    ///
    /// # Returns
    ///
    /// The value at the index, or None if the index is out of bounds
    #[must_use]
    pub fn get_by_index(&self, index: usize) -> Option<&RowValues> {
        self.rows.get(index)
    }
}
