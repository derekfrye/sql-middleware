use crate::plan::{Action, Interaction};

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskState {
    pub(super) has_conn: bool,
    pub(super) in_tx: bool,
}

#[derive(Debug, Clone)]
pub(super) struct GenState {
    pub(super) next_id: i64,
}

pub(super) fn apply_generated_action(
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
