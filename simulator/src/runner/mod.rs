use std::path::Path;

use crate::args::{BackendKind, ResetMode};
use crate::backends::{Backend, BackendError, ErrorClass};
use crate::plan::{Action, ErrorExpectation, Plan, QueryExpectation};
use sql_middleware::{ResultSet, RowValues};

#[derive(Debug)]
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
struct TaskState {
    conn: Option<sql_middleware::MiddlewarePoolConnection>,
    in_tx: bool,
}

pub(crate) fn load_plan(path: &Path) -> Result<Plan, String> {
    Plan::from_json_path(path)
}

pub(crate) async fn apply_reset(
    backend: &mut Box<dyn Backend>,
    backend_kind: BackendKind,
    reset_mode: ResetMode,
    tables: &[String],
) -> Result<(), BackendError> {
    if tables.is_empty() {
        return Ok(());
    }
    let mut conn = backend.checkout().await?;
    for statement in reset_statements(backend_kind, reset_mode, tables) {
        backend.execute(&mut conn, &statement, false).await?;
    }
    drop(conn);
    Ok(())
}

pub(crate) async fn execute_plan(plan: Plan, backend: &mut Box<dyn Backend>) -> PlanRun {
    let task_count = plan
        .interactions
        .iter()
        .map(|interaction| interaction.task)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0);

    let mut tasks = Vec::with_capacity(task_count);
    for _ in 0..task_count {
        tasks.push(TaskState::default());
    }

    let mut outcomes = Vec::with_capacity(plan.interactions.len());
    let mut error = None;

    for (step, interaction) in plan.interactions.into_iter().enumerate() {
        let task_id = interaction.task;
        let action = interaction.action;

        let task = match tasks.get_mut(task_id) {
            Some(task) => task,
            None => {
                let run_error = RunError {
                    step,
                    task: task_id,
                    action: action.clone(),
                    reason: "unknown task id".to_string(),
                };
                outcomes.push(StepOutcome {
                    step,
                    task: task_id,
                    action: action.clone(),
                    result: StepResult::Err(ActionError {
                        class: ErrorClass::Init,
                        message: run_error.reason.clone(),
                    }),
                });
                error = Some(run_error);
                break;
            }
        };

        match apply_action(backend, task, &action).await {
            Ok(observation) => {
                outcomes.push(StepOutcome {
                    step,
                    task: task_id,
                    action: action.clone(),
                    result: StepResult::Ok(observation),
                });
            }
            Err(err) => {
                let action_error = ActionError {
                    class: err.class(),
                    message: err.to_string(),
                };
                outcomes.push(StepOutcome {
                    step,
                    task: task_id,
                    action: action.clone(),
                    result: StepResult::Err(action_error),
                });
                error = Some(RunError {
                    step,
                    task: task_id,
                    action: action.clone(),
                    reason: err.to_string(),
                });
                break;
            }
        }

        tracing::info!(
            "plan_step={} task={} action={}",
            step,
            task_id,
            action_label(&action)
        );
    }

    PlanRun { outcomes, error }
}

pub(crate) async fn run_plan(
    plan: Plan,
    backend: &mut Box<dyn Backend>,
) -> Result<RunSummary, RunError> {
    let run = execute_plan(plan, backend).await;
    if let Some(err) = run.error {
        Err(err)
    } else {
        Ok(RunSummary {
            steps: run.outcomes.len(),
        })
    }
}

async fn apply_action(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
    action: &Action,
) -> Result<ActionObservation, BackendError> {
    match action {
        Action::Checkout => {
            if task.conn.is_some() {
                return Err(BackendError::Init(
                    "checkout requested while task already has a connection".to_string(),
                ));
            }
            let conn = backend.checkout().await?;
            task.conn = Some(conn);
            task.in_tx = false;
            Ok(ActionObservation::Simple)
        }
        Action::Return => {
            if task.in_tx {
                return Err(BackendError::Init(
                    "return requested while task is in a transaction".to_string(),
                ));
            }
            let conn = task.conn.take().ok_or_else(|| {
                BackendError::Init("return requested without a connection".to_string())
            })?;
            drop(conn);
            Ok(ActionObservation::Simple)
        }
        Action::Begin => {
            if task.in_tx {
                return Err(BackendError::Init(
                    "begin requested while already in a transaction".to_string(),
                ));
            }
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("begin requested without a connection".to_string())
            })?;
            backend.begin(conn).await?;
            task.in_tx = true;
            Ok(ActionObservation::Simple)
        }
        Action::Commit => {
            if !task.in_tx {
                return Err(BackendError::Init(
                    "commit requested without an active transaction".to_string(),
                ));
            }
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("commit requested without a connection".to_string())
            })?;
            backend.commit(conn).await?;
            task.in_tx = false;
            Ok(ActionObservation::Simple)
        }
        Action::Rollback => {
            if !task.in_tx {
                return Err(BackendError::Init(
                    "rollback requested without an active transaction".to_string(),
                ));
            }
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("rollback requested without a connection".to_string())
            })?;
            backend.rollback(conn).await?;
            task.in_tx = false;
            Ok(ActionObservation::Simple)
        }
        Action::Execute { sql, expect_error } => {
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("execute requested without a connection".to_string())
            })?;
            let result = backend.execute(conn, sql, task.in_tx).await;
            handle_action_result(result, expect_error)?;
            Ok(ActionObservation::Simple)
        }
        Action::Query {
            sql,
            expect,
            expect_error,
        } => {
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("query requested without a connection".to_string())
            })?;
            let result = backend.query(conn, sql, task.in_tx).await;
            let result = match handle_action_result(result, expect_error)? {
                Some(result) => result,
                None => return Ok(ActionObservation::Simple),
            };
            let summary = summarize_result(&result);
            if let Some(expect) = expect {
                verify_query_expectation(expect, &summary)?;
            }
            tracing::info!(
                "plan_query rows={} columns={}",
                summary.row_count,
                summary.column_count
            );
            Ok(ActionObservation::Query(normalize_result(&result, summary)))
        }
        Action::Sleep { ms } => {
            backend.sleep(*ms).await;
            Ok(ActionObservation::Simple)
        }
    }
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Checkout => "checkout",
        Action::Return => "return",
        Action::Begin => "begin",
        Action::Commit => "commit",
        Action::Rollback => "rollback",
        Action::Execute { .. } => "execute",
        Action::Query { .. } => "query",
        Action::Sleep { .. } => "sleep",
    }
}

fn summarize_result(result: &ResultSet) -> QuerySummary {
    let columns = extract_column_names(result);
    QuerySummary {
        row_count: result.results.len(),
        column_count: columns.len(),
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
        RowValues::Timestamp(val) => {
            format!("t:{}", val.format("%Y-%m-%dT%H:%M:%S%.6f"))
        }
        RowValues::Null => "null".to_string(),
        RowValues::JSON(val) => format!("j:{val}"),
        RowValues::Blob(bytes) => format!("blob:{}", hex_encode(bytes)),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    use std::fmt::Write;
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn reset_statements(
    backend_kind: BackendKind,
    reset_mode: ResetMode,
    tables: &[String],
) -> Vec<String> {
    tables
        .iter()
        .map(|table| match reset_mode {
            ResetMode::Delete => format!("DELETE FROM {table};"),
            ResetMode::Truncate => match backend_kind {
                BackendKind::Sqlite | BackendKind::Turso => format!("DELETE FROM {table};"),
                BackendKind::Postgres => format!("TRUNCATE TABLE {table};"),
            },
            ResetMode::Recreate => format!("DROP TABLE IF EXISTS {table};"),
        })
        .collect()
}

fn handle_action_result<T>(
    result: Result<T, BackendError>,
    expect_error: &Option<ErrorExpectation>,
) -> Result<Option<T>, BackendError> {
    match (result, expect_error) {
        (Ok(value), None) => Ok(Some(value)),
        (Ok(_), Some(expect)) => Err(BackendError::Init(format!(
            "expected error containing {:?}, but action succeeded",
            expect.contains
        ))),
        (Err(err), None) => Err(err),
        (Err(err), Some(expect)) => {
            if error_matches(&err, expect) {
                Ok(None)
            } else {
                Err(BackendError::Init(format!(
                    "error mismatch: expected {:?}, got {}",
                    expect.contains, err
                )))
            }
        }
    }
}

fn error_matches(err: &BackendError, expect: &ErrorExpectation) -> bool {
    err.to_string().contains(&expect.contains)
}

fn verify_query_expectation(
    expect: &QueryExpectation,
    summary: &QuerySummary,
) -> Result<(), BackendError> {
    if let Some(row_count) = expect.row_count {
        if summary.row_count != row_count {
            return Err(BackendError::Init(format!(
                "query row_count mismatch: expected {row_count}, got {}",
                summary.row_count
            )));
        }
    }
    if let Some(column_count) = expect.column_count {
        if summary.column_count != column_count {
            return Err(BackendError::Init(format!(
                "query column_count mismatch: expected {column_count}, got {}",
                summary.column_count
            )));
        }
    }
    Ok(())
}
