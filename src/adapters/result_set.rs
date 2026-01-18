use std::sync::Arc;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};

pub(crate) fn init_result_set(column_names: Vec<String>, capacity: usize) -> ResultSet {
    let mut result_set = ResultSet::with_capacity(capacity);
    result_set.set_column_names(Arc::new(column_names));
    result_set
}

pub(crate) fn column_count(result_set: &ResultSet) -> Result<usize, SqlMiddlewareDbError> {
    result_set
        .get_column_names()
        .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("No column names available".to_string()))
        .map(|cols| cols.len())
}
