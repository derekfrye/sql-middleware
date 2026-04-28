use crate::backends::BackendError;
use crate::plan::QueryExpectation;
use sql_middleware::{ResultSet, RowValues};
use std::fmt::Write;

use super::types::{QueryObservation, QuerySummary};

pub(super) fn observe_query_result(
    result: &ResultSet,
    expect: Option<&QueryExpectation>,
) -> Result<QueryObservation, BackendError> {
    let summary = summarize_result(result);
    if let Some(expect) = expect {
        verify_query_expectation(expect, &summary)?;
    }
    tracing::info!(
        "plan_query rows={} columns={}",
        summary.row_count,
        summary.column_count
    );
    Ok(normalize_result(result, summary))
}

fn summarize_result(result: &ResultSet) -> QuerySummary {
    QuerySummary {
        row_count: result.results.len(),
        column_count: extract_column_names(result).len(),
    }
}

fn normalize_result(result: &ResultSet, summary: QuerySummary) -> QueryObservation {
    let columns = extract_column_names(result)
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut rows = result
        .results
        .iter()
        .map(|row| row.rows.iter().map(normalize_value).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    rows.sort();
    QueryObservation {
        summary,
        columns,
        rows,
    }
}

fn extract_column_names(result: &ResultSet) -> Vec<String> {
    if let Some(columns) = result.get_column_names() {
        columns.as_ref().clone()
    } else if let Some(row) = result.results.first() {
        row.column_names.as_ref().clone()
    } else {
        Vec::new()
    }
}

fn normalize_value(value: &RowValues) -> String {
    match value {
        RowValues::Int(val) => format!("i:{val}"),
        RowValues::Float(val) => format!("f:{val}"),
        RowValues::Text(val) => format!("s:{val}"),
        RowValues::Bool(val) => format!("b:{val}"),
        RowValues::Timestamp(val) => format!("t:{}", val.format("%Y-%m-%dT%H:%M:%S%.6f")),
        RowValues::Null => "null".to_string(),
        RowValues::JSON(val) => format!("j:{val}"),
        RowValues::Blob(bytes) => format!("blob:{}", hex_encode(bytes)),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn verify_query_expectation(
    expect: &QueryExpectation,
    summary: &QuerySummary,
) -> Result<(), BackendError> {
    if let Some(row_count) = expect.row_count
        && summary.row_count != row_count
    {
        return Err(BackendError::Init(format!(
            "query row_count mismatch: expected {row_count}, got {}",
            summary.row_count
        )));
    }
    if let Some(column_count) = expect.column_count
        && summary.column_count != column_count
    {
        return Err(BackendError::Init(format!(
            "query column_count mismatch: expected {column_count}, got {}",
            summary.column_count
        )));
    }
    Ok(())
}
