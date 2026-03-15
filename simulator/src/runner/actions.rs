use crate::backends::{Backend, BackendError};
use crate::plan::{Action, ErrorExpectation, QueryExpectation};

use super::query_observation::observe_query_result;
use super::types::{ActionObservation, TaskState};

pub(super) async fn apply_action(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
    action: &Action,
) -> Result<ActionObservation, BackendError> {
    match action {
        Action::Checkout => checkout(backend, task).await,
        Action::Return => return_connection(task),
        Action::Begin => begin(backend, task).await,
        Action::Commit => commit(backend, task).await,
        Action::Rollback => rollback(backend, task).await,
        Action::Execute { sql, expect_error } => execute(backend, task, sql, expect_error).await,
        Action::Query {
            sql,
            expect,
            expect_error,
        } => query(backend, task, sql, expect, expect_error).await,
        Action::Sleep { ms } => {
            backend.sleep(*ms).await;
            Ok(ActionObservation::Simple)
        }
    }
}

async fn checkout(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
) -> Result<ActionObservation, BackendError> {
    if task.conn.is_some() {
        return Err(BackendError::Init(
            "checkout requested while task already has a connection".to_string(),
        ));
    }
    task.conn = Some(backend.checkout().await?);
    task.in_tx = false;
    Ok(ActionObservation::Simple)
}

fn return_connection(task: &mut TaskState) -> Result<ActionObservation, BackendError> {
    if task.in_tx {
        return Err(BackendError::Init(
            "return requested while task is in a transaction".to_string(),
        ));
    }
    let conn = task
        .conn
        .take()
        .ok_or_else(|| BackendError::Init("return requested without a connection".to_string()))?;
    drop(conn);
    Ok(ActionObservation::Simple)
}

async fn begin(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
) -> Result<ActionObservation, BackendError> {
    if task.in_tx {
        return Err(BackendError::Init(
            "begin requested while already in a transaction".to_string(),
        ));
    }
    let conn = connection_mut(task, "begin")?;
    backend.begin(conn).await?;
    task.in_tx = true;
    Ok(ActionObservation::Simple)
}

async fn commit(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
) -> Result<ActionObservation, BackendError> {
    require_in_tx(task, "commit")?;
    let conn = connection_mut(task, "commit")?;
    backend.commit(conn).await?;
    task.in_tx = false;
    Ok(ActionObservation::Simple)
}

async fn rollback(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
) -> Result<ActionObservation, BackendError> {
    require_in_tx(task, "rollback")?;
    let conn = connection_mut(task, "rollback")?;
    backend.rollback(conn).await?;
    task.in_tx = false;
    Ok(ActionObservation::Simple)
}

async fn execute(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
    sql: &str,
    expect_error: &Option<ErrorExpectation>,
) -> Result<ActionObservation, BackendError> {
    let conn = connection_mut(task, "execute")?;
    let result = backend.execute(conn, sql, task.in_tx).await;
    handle_action_result(result, expect_error)?;
    Ok(ActionObservation::Simple)
}

async fn query(
    backend: &mut Box<dyn Backend>,
    task: &mut TaskState,
    sql: &str,
    expect: &Option<QueryExpectation>,
    expect_error: &Option<ErrorExpectation>,
) -> Result<ActionObservation, BackendError> {
    let conn = connection_mut(task, "query")?;
    let result = backend.query(conn, sql, task.in_tx).await;
    let result = match handle_action_result(result, expect_error)? {
        Some(result) => result,
        None => return Ok(ActionObservation::Simple),
    };

    Ok(ActionObservation::Query(observe_query_result(
        &result, expect,
    )?))
}

fn connection_mut<'a>(
    task: &'a mut TaskState,
    action: &str,
) -> Result<&'a mut sql_middleware::MiddlewarePoolConnection, BackendError> {
    task.conn
        .as_mut()
        .ok_or_else(|| BackendError::Init(format!("{action} requested without a connection")))
}

fn require_in_tx(task: &TaskState, action: &str) -> Result<(), BackendError> {
    if task.in_tx {
        Ok(())
    } else {
        Err(BackendError::Init(format!(
            "{action} requested without an active transaction"
        )))
    }
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
        (Err(err), Some(expect)) if error_matches(&err, expect) => Ok(None),
        (Err(err), Some(expect)) => Err(BackendError::Init(format!(
            "error mismatch: expected {:?}, got {}",
            expect.contains, err
        ))),
    }
}

fn error_matches(err: &BackendError, expect: &ErrorExpectation) -> bool {
    err.to_string().contains(&expect.contains)
}
