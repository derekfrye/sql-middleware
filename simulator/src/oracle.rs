use std::collections::HashMap;

use crate::model::{ConnState, PoolModel, TaskState};

pub(crate) struct Oracle;

impl Oracle {
    pub(crate) fn check(tasks: &[TaskState], pool: &PoolModel) -> Result<(), String> {
        let mut seen = HashMap::new();
        for (conn_id, slot) in pool.conns.iter().enumerate() {
            if let Some(task_id) = slot.checked_out_by {
                if slot.state == ConnState::Broken {
                    return Err(format!(
                        "conn {conn_id} is broken but checked out by task {task_id}"
                    ));
                }
                if seen.insert(conn_id, task_id).is_some() {
                    return Err(format!("conn {conn_id} checked out multiple times"));
                }
                let task = tasks.get(task_id).ok_or_else(|| format!("task {task_id} missing"))?;
                if task.conn_id != Some(conn_id) {
                    return Err(format!("task {task_id} and conn {conn_id} mismatch"));
                }
                if slot.state == ConnState::InTx && !task.in_tx {
                    return Err(format!("conn {conn_id} is in tx but task {task_id} is not"));
                }
            }
            if slot.state == ConnState::InTx && slot.checked_out_by.is_none() {
                return Err(format!("conn {conn_id} in tx without owner"));
            }
        }

        for task in tasks {
            if task.in_tx && task.conn_id.is_none() {
                return Err(format!("task {} in tx without conn", task.id));
            }
            if let Some(conn_id) = task.conn_id {
                let slot = pool
                    .conns
                    .get(conn_id)
                    .ok_or_else(|| format!("task {} references missing conn {}", task.id, conn_id))?;
                if slot.checked_out_by != Some(task.id) {
                    return Err(format!("task {} claims conn {} without ownership", task.id, conn_id));
                }
            }
        }

        Ok(())
    }
}
