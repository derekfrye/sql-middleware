mod ops;
mod state;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::args::{BackendKind, SimConfig};
use crate::plan::{Action, Interaction, Plan};
use crate::properties::PropertyKind;
use ops::{build_action, next_op};
use state::{GenState, TaskState, apply_generated_action};

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
