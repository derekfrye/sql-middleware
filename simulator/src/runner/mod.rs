use std::path::Path;

use crate::backends::sqlite::{BackendError, SqliteBackend, SqliteBackendConfig};
use crate::plan::{Action, Plan};
use sql_middleware::ResultSet;

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

#[derive(Debug, Default)]
struct TaskState {
    conn: Option<sql_middleware::MiddlewarePoolConnection>,
    in_tx: bool,
}

pub(crate) async fn run_plan_sqlite(plan: Plan, pool_size: usize) -> Result<RunSummary, RunError> {
    let backend = SqliteBackend::new(SqliteBackendConfig::in_memory(pool_size))
        .await
        .map_err(|err| RunError {
            step: 0,
            task: 0,
            action: Action::Sleep { ms: 0 },
            reason: format!("backend init failed: {err}"),
        })?;
    run_plan(plan, backend).await
}

pub(crate) fn load_plan(path: &Path) -> Result<Plan, String> {
    Plan::from_json_path(path)
}

async fn run_plan(plan: Plan, backend: SqliteBackend) -> Result<RunSummary, RunError> {
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
    let mut backend = backend;

    let total_steps = plan.interactions.len();

    for (step, interaction) in plan.interactions.into_iter().enumerate() {
        let task_id = interaction.task;
        let action = interaction.action;

        let task = tasks.get_mut(task_id).ok_or_else(|| RunError {
            step,
            task: task_id,
            action: action.clone(),
            reason: "unknown task id".to_string(),
        })?;

        if let Err(err) = apply_action(&mut backend, task, &action).await {
            return Err(RunError {
                step,
                task: task_id,
                action: action.clone(),
                reason: err.to_string(),
            });
        }

        tracing::info!(
            "plan_step={} task={} action={}",
            step,
            task_id,
            action_label(&action)
        );
    }

    Ok(RunSummary { steps: total_steps })
}

async fn apply_action(
    backend: &mut SqliteBackend,
    task: &mut TaskState,
    action: &Action,
) -> Result<(), BackendError> {
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
        }
        Action::Execute { sql } => {
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("execute requested without a connection".to_string())
            })?;
            backend.execute(conn, sql).await?;
        }
        Action::Query { sql } => {
            let conn = task.conn.as_mut().ok_or_else(|| {
                BackendError::Init("query requested without a connection".to_string())
            })?;
            let result = backend.query(conn, sql).await?;
            let summary = summarize_result(&result);
            tracing::info!(
                "plan_query rows={} columns={}",
                summary.row_count,
                summary.column_count
            );
        }
        Action::Sleep { ms } => {
            backend.sleep(*ms).await;
        }
    }
    Ok(())
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

struct QuerySummary {
    row_count: usize,
    column_count: usize,
}

fn summarize_result(result: &ResultSet) -> QuerySummary {
    let column_count = result
        .get_column_names()
        .map(|columns| columns.len())
        .unwrap_or(0);
    QuerySummary {
        row_count: result.results.len(),
        column_count,
    }
}
