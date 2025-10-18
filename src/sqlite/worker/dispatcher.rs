use std::sync::Arc;
use std::sync::mpsc::Receiver;

use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite::{self, ToSql};

use crate::middleware::{ResultSet, SqlMiddlewareDbError};
use crate::sqlite::query::build_result_set;

use super::channel::{BoxedCallback, BoxedResponse, Command};

pub(super) fn run_sqlite_worker(object: &Object, receiver: &Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Shutdown => break,
            other => dispatch_command(object, other),
        }
    }
}

fn dispatch_command(object: &Object, command: Command) {
    match command {
        Command::ExecuteBatch { query, respond_to } => {
            let _ = respond_to.send(execute_batch(object, &query));
        }
        Command::ExecuteSelect {
            query,
            params,
            respond_to,
        } => {
            let _ = respond_to.send(execute_select(object, &query, &params));
        }
        Command::ExecuteDml {
            query,
            params,
            respond_to,
        } => {
            let _ = respond_to.send(execute_dml(object, &query, &params));
        }
        Command::PrepareStatement { query, respond_to } => {
            let _ = respond_to.send(prepare_statement(object, &query));
        }
        Command::ExecutePreparedSelect {
            query,
            params,
            respond_to,
        } => {
            let _ = respond_to.send(execute_prepared_select(object, &query, &params));
        }
        Command::ExecutePreparedDml {
            query,
            params,
            respond_to,
        } => {
            let _ = respond_to.send(execute_prepared_dml(object, &query, &params));
        }
        Command::WithConnection {
            callback,
            respond_to,
        } => {
            let _ = respond_to.send(run_custom_callback(object, callback));
        }
        Command::Shutdown => {}
    }
}

fn execute_batch(object: &Object, query: &str) -> Result<(), SqlMiddlewareDbError> {
    with_connection(object, |conn| {
        let tx = conn.transaction()?;
        tx.execute_batch(query)?;
        tx.commit()?;
        Ok(())
    })
}

fn execute_select(
    object: &Object,
    query: &str,
    params: &[rusqlite::types::Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    with_connection(object, |conn| {
        let mut stmt = conn.prepare(query)?;
        build_result_set(&mut stmt, params)
    })
}

fn execute_dml(
    object: &Object,
    query: &str,
    params: &[rusqlite::types::Value],
) -> Result<usize, SqlMiddlewareDbError> {
    transactional_dml(object, params, move |tx, param_refs| {
        let mut stmt = tx.prepare(query)?;
        Ok(stmt.execute(param_refs)?)
    })
}

fn prepare_statement(object: &Object, query: &Arc<String>) -> Result<(), SqlMiddlewareDbError> {
    with_connection(object, |conn| {
        let _ = conn.prepare_cached(query.as_ref())?;
        Ok(())
    })
}

fn execute_prepared_select(
    object: &Object,
    query: &Arc<String>,
    params: &[rusqlite::types::Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    with_connection(object, |conn| {
        let mut stmt = conn.prepare_cached(query.as_ref())?;
        build_result_set(&mut stmt, params)
    })
}

fn execute_prepared_dml(
    object: &Object,
    query: &Arc<String>,
    params: &[rusqlite::types::Value],
) -> Result<usize, SqlMiddlewareDbError> {
    transactional_dml(object, params, move |tx, param_refs| {
        let mut stmt = tx.prepare_cached(query.as_ref())?;
        Ok(stmt.execute(param_refs)?)
    })
}

fn run_custom_callback(object: &Object, callback: BoxedCallback) -> BoxedResponse {
    with_connection(object, callback)
}

fn transactional_dml<F>(
    object: &Object,
    params: &[rusqlite::types::Value],
    action: F,
) -> Result<usize, SqlMiddlewareDbError>
where
    F: FnOnce(&rusqlite::Transaction<'_>, &[&dyn ToSql]) -> Result<usize, SqlMiddlewareDbError>,
{
    with_connection(object, move |conn| {
        let tx = conn.transaction()?;
        let param_refs = values_as_tosql(params);
        let rows = action(&tx, &param_refs)?;
        tx.commit()?;
        Ok(rows)
    })
}

fn values_as_tosql(values: &[rusqlite::types::Value]) -> Vec<&dyn ToSql> {
    values.iter().map(|value| value as &dyn ToSql).collect()
}

fn with_connection<T, F>(object: &Object, f: F) -> Result<T, SqlMiddlewareDbError>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<T, SqlMiddlewareDbError>,
{
    let mut guard = object.lock().map_err(|err| {
        SqlMiddlewareDbError::ConnectionError(format!("SQLite connection mutex poisoned: {err}"))
    })?;
    f(&mut guard)
}
