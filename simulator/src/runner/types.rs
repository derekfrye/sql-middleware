use crate::backends::ErrorClass;
use crate::plan::Action;

#[derive(Debug, Clone)]
pub(crate) struct RunError {
    pub(crate) step: usize,
    pub(crate) task: usize,
    pub(crate) action: Action,
    pub(crate) reason: String,
}

#[derive(Debug)]
pub(crate) struct RunSummary {
    pub(crate) steps: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ActionError {
    pub(crate) class: ErrorClass,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ActionObservation {
    Simple,
    Query(QueryObservation),
}

#[derive(Debug, Clone)]
pub(crate) struct QueryObservation {
    pub(crate) summary: QuerySummary,
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) enum StepResult {
    Ok(ActionObservation),
    Err(ActionError),
}

#[derive(Debug, Clone)]
pub(crate) struct StepOutcome {
    pub(crate) step: usize,
    pub(crate) task: usize,
    pub(crate) action: Action,
    pub(crate) result: StepResult,
}

#[derive(Debug, Clone)]
pub(crate) struct PlanRun {
    pub(crate) outcomes: Vec<StepOutcome>,
    pub(crate) error: Option<RunError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuerySummary {
    pub(crate) row_count: usize,
    pub(crate) column_count: usize,
}

#[derive(Debug, Default)]
pub(crate) struct TaskState {
    pub(crate) conn: Option<sql_middleware::MiddlewarePoolConnection>,
    pub(crate) in_tx: bool,
}
