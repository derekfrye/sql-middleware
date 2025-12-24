use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::args::SimConfig;
use crate::backend::BackendShim;
use crate::logging::EventLog;
use crate::model::{Op, TaskState};
use crate::oracle::Oracle;
use crate::scheduler::Scheduler;

pub(crate) fn run(config: SimConfig, rng: &mut ChaCha8Rng) {
    let mut backend = BackendShim::new(config.clone());
    let mut tasks: Vec<TaskState> = (0..config.tasks)
        .map(|id| TaskState {
            id,
            conn_id: None,
            in_tx: false,
        })
        .collect();
    let mut scheduler = Scheduler::new(config.tasks);
    let mut events = EventLog::new(config.first_steps, config.tail_steps);

    let max_steps = config.iterations.unwrap_or(u64::MAX);
    let max_time = config.duration_ms.unwrap_or(u64::MAX);

    let mut step: u64 = 0;
    while step < max_steps && scheduler.clock.now_ms <= max_time {
        let task_id = match scheduler.next_ready(rng) {
            Some(id) => id,
            None => break,
        };
        let in_flight_tx = tasks.iter().filter(|t| t.in_tx).count();
        let op = next_op(&tasks[task_id], in_flight_tx, &config, rng);
        let op_display = format_op(&op);
        let outcome = backend.apply(&mut tasks[task_id], op.clone(), rng);

        match outcome {
            Ok(step_outcome) => {
                if let Op::Sleep(ms) = op {
                    scheduler.sleep(task_id, ms);
                } else {
                    scheduler.mark_ready(task_id);
                }
                let result_label = match step_outcome.result {
                    Ok(()) => "Ok".to_string(),
                    Err(ref err) => format!("Err({err:?})"),
                };
                let conn_label = step_outcome
                    .conn_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let message = format!(
                    "step={} time={}ms task={} op={} conn={} result={}",
                    step,
                    scheduler.clock.now_ms,
                    task_id,
                    op_display,
                    conn_label,
                    result_label
                );
                events.record(message);
            }
            Err(reason) => {
                events.dump_failure(&reason);
                std::process::exit(1);
            }
        }

        if let Err(reason) = Oracle::check(&tasks, &backend.pool) {
            events.dump_failure(&reason);
            std::process::exit(1);
        }
        scheduler.advance_time(1);
        step += 1;
    }

    let summary = format!(
        "complete: steps={} time={}ms tasks={} pool_size={}",
        step,
        scheduler.clock.now_ms,
        config.tasks,
        config.pool_size
    );
    tracing::info!("{}", summary);
}

fn next_op(task: &TaskState, in_flight_tx: usize, config: &SimConfig, rng: &mut ChaCha8Rng) -> Op {
    if rng.random::<f64>() < config.sleep_rate {
        return Op::Sleep(rng.random_range(1..=50));
    }

    if task.conn_id.is_none() {
        return Op::Checkout;
    }

    if task.in_tx {
        let weights = [
            (Op::Execute, 0.45),
            (Op::Select, 0.25),
            (Op::Commit, 0.15),
            (Op::Rollback, 0.10),
            (Op::Ddl, config.ddl_rate),
        ];
        return choose_weighted(&weights, rng);
    }

    let mut weights = vec![
        (Op::Execute, 0.35),
        (Op::Select, 0.25),
        (Op::Return, 0.15),
        (Op::Ddl, config.ddl_rate),
    ];
    if in_flight_tx < config.max_in_flight_tx {
        weights.push((Op::Begin, 0.20));
    }
    choose_weighted(&weights, rng)
}

fn choose_weighted(items: &[(Op, f64)], rng: &mut ChaCha8Rng) -> Op {
    let total: f64 = items.iter().map(|(_, weight)| weight.max(0.0)).sum();
    if total <= f64::EPSILON {
        return items
            .first()
            .map(|(op, _)| op.clone())
            .unwrap_or(Op::Sleep(1));
    }
    let mut target = rng.random::<f64>() * total;
    for (op, weight) in items {
        let w = weight.max(0.0);
        if target <= w {
            return op.clone();
        }
        target -= w;
    }
    items
        .last()
        .map(|(op, _)| op.clone())
        .unwrap_or(Op::Sleep(1))
}

fn format_op(op: &Op) -> String {
    match op {
        Op::Sleep(ms) => format!("Sleep({}ms)", ms),
        other => format!("{:?}", other),
    }
}
