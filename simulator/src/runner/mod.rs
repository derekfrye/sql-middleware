mod actions;
mod query_observation;
mod types;

use std::path::Path;

use crate::args::{BackendKind, ResetMode};
use crate::backends::{Backend, BackendError, ErrorClass};
use crate::plan::{Action, Plan};

use types::TaskState;
pub(crate) use types::{
    ActionError, ActionObservation, PlanRun, QueryObservation, RunError, RunSummary, StepOutcome,
    StepResult,
};

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
        .map_or(0, |id| id + 1);

    let mut tasks = Vec::with_capacity(task_count);
    for _ in 0..task_count {
        tasks.push(TaskState::default());
    }

    let mut outcomes = Vec::with_capacity(plan.interactions.len());
    let mut error = None;

    for (step, interaction) in plan.interactions.into_iter().enumerate() {
        let task_id = interaction.task;
        let action = interaction.action;

        let Some(task) = tasks.get_mut(task_id) else {
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
        };

        match actions::apply_action(backend, task, &action).await {
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
