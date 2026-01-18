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
    Execute { sql: String },
    Query { sql: String },
    Sleep { ms: u64 },
}

impl Plan {
    pub(crate) fn from_json_path(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|err| format!("failed to read plan file {}: {err}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|err| format!("failed to parse plan JSON {}: {err}", path.display()))
    }
}
