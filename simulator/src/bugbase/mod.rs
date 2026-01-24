use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::args::SimConfig;
use crate::plan::{Action, Plan};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BugRecordKind {
    PlanRun,
    Compare,
    Setup,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BugRecord {
    pub(crate) kind: BugRecordKind,
    pub(crate) label: String,
    pub(crate) error: String,
    pub(crate) step: Option<usize>,
    pub(crate) task: Option<usize>,
    pub(crate) action: Option<Action>,
    pub(crate) comparison_step: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct BugRecordWithTimestamp {
    timestamp: String,
    #[serde(flatten)]
    record: BugRecord,
}

#[derive(Debug)]
pub(crate) struct BugBase {
    root: PathBuf,
}

impl BugBase {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn store_failure(
        &self,
        config: &SimConfig,
        plan: &Plan,
        shrunk_plan: Option<&Plan>,
        record: &BugRecord,
    ) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.root)
            .map_err(|err| format!("failed to create bugbase dir: {err}"))?;
        let dir = create_unique_dir(&self.root, "bug")?;

        write_json(&dir.join("plan.json"), plan)?;
        write_json(&dir.join("config.json"), config)?;
        let record_with_timestamp = BugRecordWithTimestamp {
            timestamp: current_timestamp(),
            record: record.clone(),
        };
        write_json(&dir.join("failure.json"), &record_with_timestamp)?;
        if let Some(shrunk_plan) = shrunk_plan {
            write_json(&dir.join("shrunk_plan.json"), shrunk_plan)?;
        }
        Ok(dir)
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|err| format!("failed to serialize {}: {err}", path.display()))?;
    fs::write(path, content)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn create_unique_dir(root: &Path, prefix: &str) -> Result<PathBuf, String> {
    let base = format!("{prefix}-{}", current_timestamp());
    let mut attempt = 0u32;
    loop {
        let name = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let dir = root.join(name);
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt = attempt.saturating_add(1);
                continue;
            }
            Err(err) => {
                return Err(format!(
                    "failed to create bugbase entry {}: {err}",
                    dir.display()
                ))
            }
        }
    }
}

fn current_timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}-{:06}", now.as_secs(), now.subsec_micros())
}
