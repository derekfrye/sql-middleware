use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Plan {
    pub(crate) interactions: Vec<Interaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Interaction {
    pub(crate) task: usize,
    pub(crate) action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Action {
    Checkout,
    Return,
    Begin,
    Commit,
    Rollback,
    Execute {
        sql: String,
        expect_error: Option<ErrorExpectation>,
    },
    Query {
        sql: String,
        expect: Option<QueryExpectation>,
        expect_error: Option<ErrorExpectation>,
    },
    Sleep { ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct QueryExpectation {
    pub(crate) row_count: Option<usize>,
    pub(crate) column_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ErrorExpectation {
    pub(crate) contains: String,
}

impl Plan {
    pub(crate) fn from_json_path(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|err| format!("failed to read plan file {}: {err}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|err| format!("failed to parse plan JSON {}: {err}", path.display()))
    }
}
