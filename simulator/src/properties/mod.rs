use clap::ValueEnum;
use serde::Serialize;

use crate::plan::{Action, Interaction, Plan, QueryExpectation};

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
pub(crate) enum PropertyKind {
    PoolCheckoutReturn,
    TxCommitVisible,
}

impl PropertyKind {
    pub(crate) fn build_plan(self) -> Plan {
        match self {
            PropertyKind::PoolCheckoutReturn => pool_checkout_return_plan(),
            PropertyKind::TxCommitVisible => tx_commit_visible_plan(),
        }
    }
}

fn pool_checkout_return_plan() -> Plan {
    let table = "sim_pool_checkout_return";
    Plan {
        interactions: vec![
            interaction(0, Action::Checkout),
            interaction(
                0,
                Action::Execute {
                    sql: format!("CREATE TABLE IF NOT EXISTS {table} (id INTEGER);"),
                },
            ),
            interaction(
                0,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (1);"),
                },
            ),
            interaction(0, Action::Return),
            interaction(1, Action::Checkout),
            interaction(
                1,
                Action::Query {
                    sql: format!("SELECT id FROM {table} ORDER BY id;"),
                    expect: Some(QueryExpectation {
                        row_count: Some(1),
                        column_count: Some(1),
                    }),
                },
            ),
            interaction(1, Action::Return),
        ],
    }
}

fn tx_commit_visible_plan() -> Plan {
    let table = "sim_tx_commit_visible";
    Plan {
        interactions: vec![
            interaction(0, Action::Checkout),
            interaction(
                0,
                Action::Execute {
                    sql: format!("CREATE TABLE IF NOT EXISTS {table} (id INTEGER);"),
                },
            ),
            interaction(0, Action::Begin),
            interaction(
                0,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (1);"),
                },
            ),
            interaction(0, Action::Commit),
            interaction(0, Action::Return),
            interaction(1, Action::Checkout),
            interaction(
                1,
                Action::Query {
                    sql: format!("SELECT id FROM {table} ORDER BY id;"),
                    expect: Some(QueryExpectation {
                        row_count: Some(1),
                        column_count: Some(1),
                    }),
                },
            ),
            interaction(1, Action::Return),
        ],
    }
}

fn interaction(task: usize, action: Action) -> Interaction {
    Interaction { task, action }
}
