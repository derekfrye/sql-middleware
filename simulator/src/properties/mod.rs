use clap::ValueEnum;
use serde::Serialize;

use crate::plan::{Action, ErrorExpectation, Interaction, Plan, QueryExpectation};

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
pub(crate) enum PropertyKind {
    PoolCheckoutReturn,
    TxCommitVisible,
    TxRollbackInvisible,
    RetryAfterBusy,
}

impl PropertyKind {
    pub(crate) fn build_plan(self) -> Plan {
        match self {
            PropertyKind::PoolCheckoutReturn => pool_checkout_return_plan(),
            PropertyKind::TxCommitVisible => tx_commit_visible_plan(),
            PropertyKind::TxRollbackInvisible => tx_rollback_invisible_plan(),
            PropertyKind::RetryAfterBusy => retry_after_busy_plan(),
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
                    expect_error: None,
                },
            ),
            interaction(
                0,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (1);"),
                    expect_error: None,
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
                    expect_error: None,
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
                    expect_error: None,
                },
            ),
            interaction(0, Action::Begin),
            interaction(
                0,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (1);"),
                    expect_error: None,
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
                    expect_error: None,
                },
            ),
            interaction(1, Action::Return),
        ],
    }
}

fn tx_rollback_invisible_plan() -> Plan {
    let table = "sim_tx_rollback_invisible";
    Plan {
        interactions: vec![
            interaction(0, Action::Checkout),
            interaction(
                0,
                Action::Execute {
                    sql: format!("CREATE TABLE IF NOT EXISTS {table} (id INTEGER);"),
                    expect_error: None,
                },
            ),
            interaction(0, Action::Begin),
            interaction(
                0,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (1);"),
                    expect_error: None,
                },
            ),
            interaction(0, Action::Rollback),
            interaction(0, Action::Return),
            interaction(1, Action::Checkout),
            interaction(
                1,
                Action::Query {
                    sql: format!("SELECT id FROM {table} ORDER BY id;"),
                    expect: Some(QueryExpectation {
                        row_count: Some(0),
                        column_count: Some(1),
                    }),
                    expect_error: None,
                },
            ),
            interaction(1, Action::Return),
        ],
    }
}

fn retry_after_busy_plan() -> Plan {
    let table = "sim_retry_after_busy";
    Plan {
        interactions: vec![
            interaction(0, Action::Checkout),
            interaction(
                0,
                Action::Execute {
                    sql: format!("CREATE TABLE IF NOT EXISTS {table} (id INTEGER);"),
                    expect_error: None,
                },
            ),
            interaction(
                0,
                Action::Execute {
                    sql: "BEGIN IMMEDIATE;".to_string(),
                    expect_error: None,
                },
            ),
            interaction(
                0,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (1);"),
                    expect_error: None,
                },
            ),
            interaction(1, Action::Checkout),
            interaction(
                1,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (2);"),
                    expect_error: Some(ErrorExpectation {
                        contains: "locked".to_string(),
                    }),
                },
            ),
            interaction(0, Action::Commit),
            interaction(
                1,
                Action::Execute {
                    sql: format!("INSERT INTO {table} (id) VALUES (2);"),
                    expect_error: None,
                },
            ),
            interaction(
                1,
                Action::Query {
                    sql: format!("SELECT COUNT(*) FROM {table};"),
                    expect: Some(QueryExpectation {
                        row_count: Some(1),
                        column_count: Some(1),
                    }),
                    expect_error: None,
                },
            ),
            interaction(1, Action::Return),
            interaction(0, Action::Return),
        ],
    }
}

fn interaction(task: usize, action: Action) -> Interaction {
    Interaction { task, action }
}
