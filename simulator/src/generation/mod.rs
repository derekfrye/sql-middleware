use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::args::SimConfig;
use crate::plan::{Action, Interaction, Plan};
use crate::properties::PropertyKind;

#[derive(Debug, Clone, Copy)]
struct TaskState {
    has_conn: bool,
    in_tx: bool,
}

#[derive(Debug, Clone)]
struct GenState {
    next_id: i64,
}

pub(crate) fn generate_plan(config: &SimConfig) -> Result<Plan, String> {
    let steps = config.steps.max(1);
    let tasks = config.tasks.max(1);
    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);

    let mut interactions = Vec::with_capacity(steps);
    let mut task_state = vec![
        TaskState {
            has_conn: false,
            in_tx: false
        };
        tasks
    ];
    let mut gen_state = GenState { next_id: 1 };

    let mut prefix = Vec::new();
    prefix.extend(bootstrap_plan());
    if let Some(property) = config.property {
        let required_tasks = property_required_tasks(property);
        if tasks < required_tasks {
            return Err(format!(
                "property {:?} requires at least {} tasks",
                property, required_tasks
            ));
        }
        prefix.extend(property.build_plan().interactions);
    }

    let prefix_len = prefix.len();
    if prefix_len >= steps {
        return Ok(Plan {
            interactions: prefix.into_iter().take(steps).collect(),
        });
    }
    interactions.extend(prefix);

    let mut in_flight_tx = 0usize;
    for action in interactions.iter() {
        apply_generated_action(&mut task_state, action, &mut in_flight_tx);
    }

    while interactions.len() < steps {
        let task_id = rng.random_range(0..tasks);
        let task = task_state
            .get(task_id)
            .ok_or_else(|| format!("missing task state for {task_id}"))?;
        let op = next_op(task, in_flight_tx, config, &mut rng);
        let action = build_action(task_id, op, &mut gen_state);
        apply_generated_action(&mut task_state, &action, &mut in_flight_tx);
        interactions.push(action);
    }

    Ok(Plan { interactions })
}

fn bootstrap_plan() -> Vec<Interaction> {
    let table = "sim_gen";
    vec![
        interaction(0, Action::Checkout),
        interaction(
            0,
            Action::Execute {
                sql: format!("CREATE TABLE IF NOT EXISTS {table} (id INTEGER, value TEXT);"),
                expect_error: None,
            },
        ),
        interaction(0, Action::Return),
    ]
}

fn property_required_tasks(property: PropertyKind) -> usize {
    match property {
        PropertyKind::PoolCheckoutReturn => 2,
        PropertyKind::TxCommitVisible => 2,
        PropertyKind::TxRollbackInvisible => 2,
        PropertyKind::RetryAfterBusy => 2,
    }
}

#[derive(Debug, Clone)]
enum GenOp {
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

fn next_op(
    task: &TaskState,
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
        let commit_weight = (0.15 - config.panic_rate).max(0.0);
        let rollback_weight = 0.10 + config.panic_rate;
        let weights = [
            (GenOp::Execute, 0.45),
            (GenOp::Query, 0.25),
            (GenOp::Commit, commit_weight),
            (GenOp::Rollback, rollback_weight),
            (GenOp::Ddl, config.ddl_rate),
        ];
        return choose_weighted(&weights, rng);
    }

    let mut weights = vec![
        (GenOp::Execute, 0.35),
        (GenOp::Query, 0.25),
        (GenOp::Return, 0.15),
        (GenOp::Ddl, config.ddl_rate),
    ];
    if in_flight_tx < config.max_in_flight_tx {
        weights.push((GenOp::Begin, 0.20));
    }
    choose_weighted(&weights, rng)
}

fn choose_weighted(items: &[(GenOp, f64)], rng: &mut ChaCha8Rng) -> GenOp {
    let total: f64 = items.iter().map(|(_, weight)| weight.max(0.0)).sum();
    if total <= f64::EPSILON {
        return items
            .first()
            .map(|(op, _)| op.clone())
            .unwrap_or(GenOp::Sleep(1));
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
        .unwrap_or(GenOp::Sleep(1))
}

fn build_action(task: usize, op: GenOp, gen_state: &mut GenState) -> Interaction {
    let action = match op {
        GenOp::Checkout => Action::Checkout,
        GenOp::Return => Action::Return,
        GenOp::Begin => Action::Begin,
        GenOp::Commit => Action::Commit,
        GenOp::Rollback => Action::Rollback,
        GenOp::Execute => {
            let id = gen_state.next_id;
            gen_state.next_id += 1;
            Action::Execute {
                sql: format!(
                    "INSERT INTO sim_gen (id, value) VALUES ({id}, 'v{id}');"
                ),
                expect_error: None,
            }
        }
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

    interaction(task, action)
}

fn apply_generated_action(
    task_state: &mut [TaskState],
    interaction: &Interaction,
    in_flight_tx: &mut usize,
) {
    let task_id = interaction.task;
    if let Some(task) = task_state.get_mut(task_id) {
        match interaction.action {
            Action::Checkout => {
                task.has_conn = true;
                task.in_tx = false;
            }
            Action::Return => {
                if task.in_tx {
                    *in_flight_tx = in_flight_tx.saturating_sub(1);
                }
                task.has_conn = false;
                task.in_tx = false;
            }
            Action::Begin => {
                task.in_tx = true;
                *in_flight_tx += 1;
            }
            Action::Commit | Action::Rollback => {
                if task.in_tx {
                    *in_flight_tx = in_flight_tx.saturating_sub(1);
                }
                task.in_tx = false;
            }
            Action::Execute { .. } | Action::Query { .. } | Action::Sleep { .. } => {}
        }
    }
}

fn interaction(task: usize, action: Action) -> Interaction {
    Interaction { task, action }
}
