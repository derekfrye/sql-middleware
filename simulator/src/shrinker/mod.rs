use std::future::Future;

use crate::plan::Plan;

#[derive(Debug, Clone)]
pub(crate) struct ShrinkReport {
    pub(crate) original_steps: usize,
    pub(crate) shrunk_steps: usize,
    pub(crate) rounds: usize,
    pub(crate) attempts: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ShrinkResult {
    pub(crate) plan: Plan,
    pub(crate) report: ShrinkReport,
}

pub(crate) async fn shrink_plan<F, Fut>(
    plan: Plan,
    max_rounds: usize,
    predicate: F,
) -> ShrinkResult
where
    F: Fn(&Plan) -> Fut,
    Fut: Future<Output = bool>,
{
    let original_steps = plan.interactions.len();
    if original_steps <= 1 {
        return ShrinkResult {
            plan,
            report: ShrinkReport {
                original_steps,
                shrunk_steps: original_steps,
                rounds: 0,
                attempts: 0,
            },
        };
    }

    let mut current = plan;
    let mut rounds = 0usize;
    let mut attempts = 0usize;
    let mut n = 2usize;

    while rounds < max_rounds {
        let total = current.interactions.len();
        if total <= 1 {
            break;
        }

        let chunk_size = (total + n - 1) / n;
        let mut reduced = false;

        for chunk_index in 0..n {
            let start = chunk_index * chunk_size;
            if start >= total {
                break;
            }
            let end = (start + chunk_size).min(total);
            let candidate = remove_range(&current, start, end);
            if candidate.interactions.is_empty() {
                continue;
            }
            attempts += 1;
            if predicate(&candidate).await {
                current = candidate;
                n = 2;
                reduced = true;
                break;
            }
        }

        rounds += 1;
        if reduced {
            continue;
        }

        if n >= total {
            break;
        }
        n = (n * 2).min(total);
    }

    let shrunk_steps = current.interactions.len();
    ShrinkResult {
        plan: current,
        report: ShrinkReport {
            original_steps,
            shrunk_steps,
            rounds,
            attempts,
        },
    }
}

fn remove_range(plan: &Plan, start: usize, end: usize) -> Plan {
    let mut interactions = Vec::with_capacity(plan.interactions.len().saturating_sub(end - start));
    interactions.extend_from_slice(&plan.interactions[..start]);
    interactions.extend_from_slice(&plan.interactions[end..]);
    Plan { interactions }
}
