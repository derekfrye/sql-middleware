use rand::RngExt;
use rand_chacha::ChaCha8Rng;

use crate::args::{BackendKind, SimConfig};
use crate::plan::{Action, Interaction};

use super::state::{GenState, TaskState};

#[derive(Debug, Clone, Copy)]
pub(super) enum GenOp {
    Checkout,
    Return,
    Begin,
    Commit,
    Rollback,
    Execute,
    Query,
    Ddl,
    Sleep(u64),
}

pub(super) fn next_op(
    task: TaskState,
    in_flight_tx: usize,
    config: &SimConfig,
    rng: &mut ChaCha8Rng,
) -> GenOp {
    if task.has_conn && rng.random::<f64>() < config.busy_rate {
        return GenOp::Sleep(rng.random_range(1..=50));
    }
    if rng.random::<f64>() < config.sleep_rate {
        return GenOp::Sleep(rng.random_range(1..=50));
    }
    if !task.has_conn {
        return GenOp::Checkout;
    }
    if task.in_tx {
        return choose_weighted(&tx_weights(config), rng);
    }
    if matches!(config.backend, BackendKind::Sqlite) && in_flight_tx > 0 {
        return GenOp::Return;
    }
    choose_weighted(&idle_weights(in_flight_tx, config), rng)
}

pub(super) fn build_action(task: usize, op: GenOp, gen_state: &mut GenState) -> Interaction {
    let action = match op {
        GenOp::Checkout => Action::Checkout,
        GenOp::Return => Action::Return,
        GenOp::Begin => Action::Begin,
        GenOp::Commit => Action::Commit,
        GenOp::Rollback => Action::Rollback,
        GenOp::Execute => insert_action(gen_state),
        GenOp::Query => Action::Query {
            sql: "SELECT id, value FROM sim_gen ORDER BY id LIMIT 5;".to_string(),
            expect: None,
            expect_error: None,
        },
        GenOp::Ddl => Action::Execute {
            sql: "CREATE TABLE IF NOT EXISTS sim_gen (id INTEGER, value TEXT);".to_string(),
            expect_error: None,
        },
        GenOp::Sleep(ms) => Action::Sleep { ms },
    };
    Interaction { task, action }
}

fn insert_action(gen_state: &mut GenState) -> Action {
    let id = gen_state.next_id;
    gen_state.next_id += 1;
    Action::Execute {
        sql: format!("INSERT INTO sim_gen (id, value) VALUES ({id}, 'v{id}');"),
        expect_error: None,
    }
}

fn tx_weights(config: &SimConfig) -> [(GenOp, f64); 5] {
    let commit_weight = (0.15 - config.panic_rate).max(0.0);
    let rollback_weight = 0.10 + config.panic_rate;
    [
        (GenOp::Execute, 0.45),
        (GenOp::Query, 0.25),
        (GenOp::Commit, commit_weight),
        (GenOp::Rollback, rollback_weight),
        (GenOp::Ddl, config.ddl_rate),
    ]
}

fn idle_weights(in_flight_tx: usize, config: &SimConfig) -> Vec<(GenOp, f64)> {
    let mut weights = vec![
        (GenOp::Execute, 0.35),
        (GenOp::Query, 0.25),
        (GenOp::Return, 0.15),
        (GenOp::Ddl, config.ddl_rate),
    ];
    if in_flight_tx < config.max_in_flight_tx {
        weights.push((GenOp::Begin, 0.20));
    }
    weights
}

fn choose_weighted(items: &[(GenOp, f64)], rng: &mut ChaCha8Rng) -> GenOp {
    let total: f64 = items.iter().map(|(_, weight)| weight.max(0.0)).sum();
    if total <= f64::EPSILON {
        return items.first().map_or(GenOp::Sleep(1), |(op, _)| *op);
    }

    let mut target = rng.random::<f64>() * total;
    for (op, weight) in items {
        let weight = weight.max(0.0);
        if target <= weight {
            return *op;
        }
        target -= weight;
    }

    items.last().map_or(GenOp::Sleep(1), |(op, _)| *op)
}
