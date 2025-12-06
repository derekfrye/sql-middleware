use crate::types::RowValues;

/// A SQL string and its bound parameters bundled together.
///
/// Handy for helpers that need to return both query text and params without
/// losing alignment with placeholder translation:
/// ```rust
/// use sql_middleware::prelude::*;
///
/// let qp = QueryAndParams::new(
///     "INSERT INTO t (id, name) VALUES ($1, $2)",
///     vec![RowValues::Int(1), RowValues::Text("alice".into())],
/// );
/// # let _ = qp;
/// ```
#[derive(Debug, Clone)]
pub struct QueryAndParams {
    /// The SQL query string
    pub query: String,
    /// The parameters to be bound to the query
    pub params: Vec<RowValues>,
}

impl QueryAndParams {
    /// Create a new `QueryAndParams` with the given query string and parameters
    ///
    /// # Arguments
    ///
    /// * `query` - The SQL query string
    /// * `params` - The parameters to bind to the query
    ///
    /// # Returns
    ///
    /// A new `QueryAndParams` instance
    pub fn new(query: impl Into<String>, params: Vec<RowValues>) -> Self {
        Self {
            query: query.into(),
            params,
        }
    }

    /// Create a new `QueryAndParams` with no parameters
    ///
    /// # Arguments
    ///
    /// * `query` - The SQL query string
    ///
    /// # Returns
    ///
    /// A new `QueryAndParams` instance with an empty parameter list
    pub fn new_without_params(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            params: Vec::new(),
        }
    }
}
