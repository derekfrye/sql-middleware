use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::args::SimConfig;
use crate::model::{ConnState, Op, PoolModel, TaskState};

#[derive(Debug, Clone)]
pub(crate) enum SimError {
    Busy,
    Io,
    Panic,
    PoolEmpty,
}

#[derive(Debug, Clone)]
pub(crate) struct StepOutcome {
    pub(crate) result: Result<(), SimError>,
    pub(crate) conn_id: Option<usize>,
}

pub(crate) struct BackendShim {
    pub(crate) pool: PoolModel,
    config: SimConfig,
}

impl BackendShim {
    pub(crate) fn new(config: SimConfig) -> Self {
        Self {
            pool: PoolModel::new(config.pool_size),
            config,
        }
    }

    pub(crate) fn apply(
        &mut self,
        task: &mut TaskState,
        op: Op,
        rng: &mut ChaCha8Rng,
    ) -> Result<StepOutcome, String> {
        match op {
            Op::Sleep(_) => Ok(StepOutcome {
                result: Ok(()),
                conn_id: task.conn_id,
            }),
            Op::Checkout => self.checkout(task, rng),
            Op::Return => self.return_conn(task),
            Op::Begin => self.begin_tx(task, rng),
            Op::Commit => self.commit_tx(task, rng),
            Op::Rollback => self.rollback_tx(task, rng),
            Op::Execute | Op::Select | Op::Ddl => self.execute(task, rng),
        }
    }

    fn checkout(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        if task.conn_id.is_some() {
            return Err("task attempted double checkout".to_string());
        }
        let available = self.pool.available_conn_ids();
        if available.is_empty() {
            return Ok(StepOutcome {
                result: Err(SimError::PoolEmpty),
                conn_id: None,
            });
        }
        let idx = rng.random_range(0..available.len());
        let conn_id = available[idx];
        let slot = &mut self.pool.conns[conn_id];
        slot.checked_out_by = Some(task.id);
        slot.state = ConnState::Idle;
        task.conn_id = Some(conn_id);
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn return_conn(&mut self, task: &mut TaskState) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task returned without a checkout".to_string())?;
        if task.in_tx {
            return Err("task returned while in transaction".to_string());
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.checked_out_by = None;
        if slot.state != ConnState::Broken {
            slot.state = ConnState::Idle;
        }
        task.conn_id = None;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn begin_tx(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task began without a checkout".to_string())?;
        if task.in_tx {
            return Err("task attempted nested begin".to_string());
        }
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::InTx;
        task.in_tx = true;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn commit_tx(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task committed without a checkout".to_string())?;
        if !task.in_tx {
            return Err("task committed without an active tx".to_string());
        }
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::Idle;
        task.in_tx = false;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn rollback_tx(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task rolled back without a checkout".to_string())?;
        if !task.in_tx {
            return Err("task rolled back without an active tx".to_string());
        }
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::Idle;
        task.in_tx = false;
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn execute(&mut self, task: &mut TaskState, rng: &mut ChaCha8Rng) -> Result<StepOutcome, String> {
        let conn_id = task.conn_id.ok_or_else(|| "task executed without a checkout".to_string())?;
        if let Some(err) = self.inject_error(task, conn_id, rng) {
            return Ok(err);
        }
        if !task.in_tx {
            let slot = &mut self.pool.conns[conn_id];
            if slot.state != ConnState::Broken {
                slot.state = ConnState::Idle;
            }
        }
        Ok(StepOutcome {
            result: Ok(()),
            conn_id: Some(conn_id),
        })
    }

    fn inject_error(
        &mut self,
        task: &mut TaskState,
        conn_id: usize,
        rng: &mut ChaCha8Rng,
    ) -> Option<StepOutcome> {
        let roll: f64 = rng.random();
        if roll < self.config.panic_rate {
            self.break_connection(task, conn_id);
            return Some(StepOutcome {
                result: Err(SimError::Panic),
                conn_id: Some(conn_id),
            });
        }
        if roll < self.config.panic_rate + self.config.busy_rate {
            if !task.in_tx {
                let slot = &mut self.pool.conns[conn_id];
                slot.state = ConnState::Busy;
            }
            return Some(StepOutcome {
                result: Err(SimError::Busy),
                conn_id: Some(conn_id),
            });
        }
        if roll < self.config.panic_rate + self.config.busy_rate + 0.01 {
            self.break_connection(task, conn_id);
            return Some(StepOutcome {
                result: Err(SimError::Io),
                conn_id: Some(conn_id),
            });
        }
        None
    }

    fn break_connection(&mut self, task: &mut TaskState, conn_id: usize) {
        let slot = &mut self.pool.conns[conn_id];
        slot.state = ConnState::Broken;
        slot.checked_out_by = None;
        task.conn_id = None;
        task.in_tx = false;
    }
}
