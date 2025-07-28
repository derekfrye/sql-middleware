use crate::types::RowValues;
use super::row::CustomDbRow;

type ColumnCacheMap = std::sync::LazyLock<
    std::sync::Mutex<
        std::collections::HashMap<
            usize,
            std::sync::Arc<std::collections::HashMap<String, usize>>,
        >,
    >,
>;

/// A result set from a database query
///
/// This struct represents the result of a database query,
/// containing the rows returned by the query and metadata.
#[derive(Debug, Clone, Default)]
pub struct ResultSet {
    /// The rows returned by the query
    pub results: Vec<CustomDbRow>,
    /// The number of rows affected (for DML statements)
    pub rows_affected: usize,
    /// Column names shared by all rows (to avoid duplicating in each row)
    column_names: Option<std::sync::Arc<Vec<String>>>,
}

impl ResultSet {
    /// Create a new result set with a known capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - The initial capacity for the result rows
    ///
    /// # Returns
    ///
    /// A new `ResultSet` instance with preallocated capacity
    #[must_use]
    pub fn with_capacity(capacity: usize) -> ResultSet {
        ResultSet {
            results: Vec::with_capacity(capacity),
            rows_affected: 0,
            column_names: None,
        }
    }

    /// Set the column names for this result set (to be shared by all rows)
    pub fn set_column_names(&mut self, column_names: std::sync::Arc<Vec<String>>) {
        self.column_names = Some(column_names);
    }

    /// Get the column names for this result set
    #[must_use]
    pub fn get_column_names(&self) -> Option<&std::sync::Arc<Vec<String>>> {
        self.column_names.as_ref()
    }

    /// Add a row to the result set
    ///
    /// # Arguments
    ///
    /// * `row_values` - The values for this row
    pub fn add_row_values(&mut self, row_values: Vec<RowValues>) {
        if let Some(column_names) = &self.column_names {
            // Build a cache of column name to index for faster lookups
            // We only need to build this cache once and reuse it
            static CACHE_MAP: ColumnCacheMap = std::sync::LazyLock::new(
                || std::sync::Mutex::new(std::collections::HashMap::new()),
            );

            // Use the pointer to column_names as a key for the cache
            let ptr = column_names.as_ref().as_ptr() as usize;
            let cache = {
                let mut cache_map = match CACHE_MAP.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        // Clear the poison and continue with the recovered data
                        poisoned.into_inner()
                    }
                };
                let cache_entry = cache_map.entry(ptr).or_insert_with(|| {
                    std::sync::Arc::new(
                        column_names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| (name.to_string(), i))
                            .collect::<std::collections::HashMap<_, _>>(),
                    )
                });
                cache_entry.clone()
            };

            let row = CustomDbRow {
                column_names: column_names.clone(),
                rows: row_values,
                column_index_cache: cache,
            };

            self.results.push(row);
            self.rows_affected += 1;
        }
    }

    /// Add a row to the result set (legacy method, less efficient)
    ///
    /// # Arguments
    ///
    /// * `row` - The row to add
    pub fn add_row(&mut self, row: CustomDbRow) {
        // If column names haven't been set yet, use the ones from this row
        if self.column_names.is_none() {
            self.column_names = Some(row.column_names.clone());
        }

        self.results.push(row);
        self.rows_affected += 1;
    }
}