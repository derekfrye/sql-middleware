use std::sync::Arc;
use std::sync::mpsc::Receiver;

use deadpool_sqlite::Object;
use deadpool_sqlite::rusqlite;

use crate::middleware::{ResultSet, SqlMiddlewareDbError};
use crate::sqlite::query::build_result_set;

use super::channel::{BoxedCallback, BoxedResponse, Command};

pub(super) fn run_sqlite_worker(object: &Object, receiver: &Receiver<Command>) {
    let mut conn_guard = match object.lock() {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("SQLite connection mutex poisoned: {err}");
            return;
        }
    };

    // Connection-level tx IDs never leave this thread; u64 won't exhaust in practice.
    let mut next_tx_id: u64 = 1;

    while let Ok(command) = receiver.recv() {
        match command {
            Command::Shutdown => break,
            Command::BeginTransaction { respond_to } => match conn_guard.transaction() {
                Ok(tx) => {
                    let tx_id = next_tx_id;
                    next_tx_id = next_tx_id.saturating_add(1);
                    let _ = respond_to.send(Ok(tx_id));
                    // Enter transaction loop until commit/rollback. We hold the rusqlite::Transaction
                    // on the worker thread because it is !Send; subsequent tx commands are routed
                    // here via tx_id until we see CommitTx/RollbackTx.
                    run_tx_loop(tx_id, tx, receiver);
                }
                Err(err) => {
                    let _ = respond_to.send(Err(SqlMiddlewareDbError::SqliteError(err)));
                }
            },
            Command::ExecuteTxBatch { respond_to, .. } => {
                let _ = respond_to.send(Err(SqlMiddlewareDbError::ExecutionError(
                    "No active SQLite transaction".into(),
                )));
            }
            Command::ExecuteTxQuery { respond_to, .. } => {
                let _ = respond_to.send(Err(SqlMiddlewareDbError::ExecutionError(
                    "No active SQLite transaction".into(),
                )));
            }
            Command::ExecuteTxDml { respond_to, .. } => {
                let _ = respond_to.send(Err(SqlMiddlewareDbError::ExecutionError(
                    "No active SQLite transaction".into(),
                )));
            }
            Command::CommitTx { respond_to, .. } => {
                let _ = respond_to.send(Err(SqlMiddlewareDbError::ExecutionError(
                    "No active SQLite transaction".into(),
                )));
            }
            Command::RollbackTx { respond_to, .. } => {
                let _ = respond_to.send(Err(SqlMiddlewareDbError::ExecutionError(
                    "No active SQLite transaction".into(),
                )));
            }
            Command::ExecuteBatch { query, respond_to } => {
                let _ = respond_to.send(execute_batch(&mut conn_guard, &query));
            }
            Command::ExecuteSelect {
                query,
                params,
                respond_to,
            } => {
                let _ = respond_to.send(execute_select(&mut conn_guard, &query, &params));
            }
            Command::ExecuteDml {
                query,
                params,
                respond_to,
            } => {
                let _ = respond_to.send(execute_dml(&mut conn_guard, &query, &params));
            }
            Command::PrepareStatement { query, respond_to } => {
                let _ = respond_to.send(prepare_statement(&mut conn_guard, &query));
            }
            Command::ExecutePreparedSelect {
                query,
                params,
                respond_to,
            } => {
                let _ = respond_to.send(execute_prepared_select(&mut conn_guard, &query, &params));
            }
            Command::ExecutePreparedDml {
                query,
                params,
                respond_to,
            } => {
                let _ = respond_to.send(execute_prepared_dml(&mut conn_guard, &query, &params));
            }
            Command::WithConnection {
                callback,
                respond_to,
            } => {
                let _ = respond_to.send(run_custom_callback(&mut conn_guard, callback));
            }
        }
    }
}

fn run_tx_loop(tx_id: u64, mut tx: rusqlite::Transaction<'_>, receiver: &Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::ExecuteTxBatch {
                tx_id: id,
                query,
                respond_to,
            } => {
                if id != tx_id {
                    let _ = respond_to.send(Err(tx_id_mismatch(tx_id, id)));
                    continue;
                }
                let res = tx.execute_batch(&query).map_err(SqlMiddlewareDbError::SqliteError);
                let _ = respond_to.send(res);
            }
            Command::ExecuteTxQuery {
                tx_id: id,
                query,
                params,
                respond_to,
            } => {
                if id != tx_id {
                    let _ = respond_to.send(Err(tx_id_mismatch(tx_id, id)));
                    continue;
                }
                let res = execute_tx_query(&mut tx, &query, &params);
                let _ = respond_to.send(res);
            }
            Command::ExecuteTxDml {
                tx_id: id,
                query,
                params,
                respond_to,
            } => {
                if id != tx_id {
                    let _ = respond_to.send(Err(tx_id_mismatch(tx_id, id)));
                    continue;
                }
                let res = execute_tx_dml(&mut tx, &query, &params);
                let _ = respond_to.send(res);
            }
            Command::CommitTx { tx_id: id, respond_to } => {
                if id != tx_id {
                    let _ = respond_to.send(Err(tx_id_mismatch(tx_id, id)));
                    continue;
                }
                let res = tx.commit().map_err(SqlMiddlewareDbError::SqliteError);
                let _ = respond_to.send(res);
                break;
            }
            Command::RollbackTx { tx_id: id, respond_to } => {
                if id != tx_id {
                    let _ = respond_to.send(Err(tx_id_mismatch(tx_id, id)));
                    continue;
                }
                let res = tx.rollback().map_err(SqlMiddlewareDbError::SqliteError);
                let _ = respond_to.send(res);
                break;
            }
            Command::Shutdown => break,
            // All other commands are blocked while a transaction is active.
            Command::BeginTransaction { respond_to } => {
                let _ = respond_to.send(Err(SqlMiddlewareDbError::ExecutionError(
                    "SQLite transaction already in progress".into(),
                )));
            }
            Command::ExecuteBatch { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
            Command::ExecuteSelect { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
            Command::ExecuteDml { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
            Command::PrepareStatement { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
            Command::ExecutePreparedSelect { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
            Command::ExecutePreparedDml { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
            Command::WithConnection { respond_to, .. } => {
                let _ = respond_to.send(Err(tx_in_progress_error()));
            }
        }
    }
}

fn tx_id_mismatch(active: u64, requested: u64) -> SqlMiddlewareDbError {
    SqlMiddlewareDbError::ExecutionError(format!(
        "SQLite transaction mismatch: active {active}, requested {requested}"
    ))
}

fn execute_batch(conn: &mut rusqlite::Connection, query: &str) -> Result<(), SqlMiddlewareDbError> {
    let tx = conn.transaction()?;
    tx.execute_batch(query)?;
    tx.commit()?;
    Ok(())
}

fn execute_select(
    conn: &mut rusqlite::Connection,
    query: &str,
    params: &[rusqlite::types::Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut stmt = conn.prepare(query)?;
    build_result_set(&mut stmt, params)
}

fn execute_dml(
    conn: &mut rusqlite::Connection,
    query: &str,
    params: &[rusqlite::types::Value],
) -> Result<usize, SqlMiddlewareDbError> {
    transactional_dml(conn, params, move |tx, param_refs| {
        let mut stmt = tx.prepare(query)?;
        Ok(stmt.execute(param_refs)?)
    })
}

fn prepare_statement(conn: &mut rusqlite::Connection, query: &Arc<String>) -> Result<(), SqlMiddlewareDbError> {
    let _ = conn.prepare_cached(query.as_ref())?;
    Ok(())
}

fn execute_prepared_select(
    conn: &mut rusqlite::Connection,
    query: &Arc<String>,
    params: &[rusqlite::types::Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut stmt = conn.prepare_cached(query.as_ref())?;
    build_result_set(&mut stmt, params)
}

fn execute_prepared_dml(
    conn: &mut rusqlite::Connection,
    query: &Arc<String>,
    params: &[rusqlite::types::Value],
) -> Result<usize, SqlMiddlewareDbError> {
    transactional_dml(conn, params, move |tx, param_refs| {
        let mut stmt = tx.prepare_cached(query.as_ref())?;
        Ok(stmt.execute(param_refs)?)
    })
}

fn execute_tx_query(
    tx: &mut rusqlite::Transaction<'_>,
    query: &Arc<String>,
    params: &[rusqlite::types::Value],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut stmt = tx.prepare_cached(query.as_ref())?;
    build_result_set(&mut stmt, params)
}

fn execute_tx_dml(
    tx: &mut rusqlite::Transaction<'_>,
    query: &Arc<String>,
    params: &[rusqlite::types::Value],
) -> Result<usize, SqlMiddlewareDbError> {
    let mut stmt = tx.prepare_cached(query.as_ref())?;
    let param_refs = values_as_tosql(params);
    Ok(stmt.execute(&param_refs[..])?)
}

fn run_custom_callback(conn: &mut rusqlite::Connection, callback: BoxedCallback) -> BoxedResponse {
    callback(conn)
}

fn transactional_dml<F>(
    conn: &mut rusqlite::Connection,
    params: &[rusqlite::types::Value],
    action: F,
) -> Result<usize, SqlMiddlewareDbError>
where
    F: FnOnce(&rusqlite::Transaction<'_>, &[&dyn rusqlite::ToSql]) -> Result<usize, SqlMiddlewareDbError>,
{
    let tx = conn.transaction()?;
    let param_refs = values_as_tosql(params);
    let rows = action(&tx, &param_refs[..])?;
    tx.commit()?;
    Ok(rows)
}

fn tx_in_progress_error() -> SqlMiddlewareDbError {
    SqlMiddlewareDbError::ExecutionError(
        "SQLite transaction in progress; operation not permitted".into(),
    )
}

fn values_as_tosql(values: &[rusqlite::types::Value]) -> Vec<&dyn rusqlite::ToSql> {
    values.iter().map(|v| v as &dyn rusqlite::ToSql).collect()
}
