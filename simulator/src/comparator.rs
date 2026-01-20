use crate::runner::{ActionObservation, PlanRun, QueryObservation, StepOutcome, StepResult};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComparisonConfig {
    pub(crate) compare_error_messages: bool,
    pub(crate) compare_query_summaries: bool,
    pub(crate) compare_query_values: bool,
}

#[derive(Debug)]
pub(crate) struct ComparisonMismatch {
    pub(crate) step: usize,
    pub(crate) reason: String,
}

pub(crate) fn compare_runs(
    left: &PlanRun,
    right: &PlanRun,
    config: ComparisonConfig,
) -> Result<(), ComparisonMismatch> {
    if left.outcomes.len() != right.outcomes.len() {
        return Err(ComparisonMismatch {
            step: left.outcomes.len().min(right.outcomes.len()),
            reason: format!(
                "step count mismatch: left={} right={}",
                left.outcomes.len(),
                right.outcomes.len()
            ),
        });
    }

    for (left_step, right_step) in left.outcomes.iter().zip(right.outcomes.iter()) {
        compare_step(left_step, right_step, config)?;
    }

    Ok(())
}

fn compare_step(
    left: &StepOutcome,
    right: &StepOutcome,
    config: ComparisonConfig,
) -> Result<(), ComparisonMismatch> {
    if left.step != right.step {
        return Err(ComparisonMismatch {
            step: left.step.min(right.step),
            reason: format!("step index mismatch: left={} right={}", left.step, right.step),
        });
    }

    if left.task != right.task {
        return Err(ComparisonMismatch {
            step: left.step,
            reason: format!("task mismatch: left={} right={}", left.task, right.task),
        });
    }

    if left.action != right.action {
        return Err(ComparisonMismatch {
            step: left.step,
            reason: format!("action mismatch: left={:?} right={:?}", left.action, right.action),
        });
    }

    match (&left.result, &right.result) {
        (StepResult::Ok(left_obs), StepResult::Ok(right_obs)) => {
            compare_observation(left.step, left_obs, right_obs, config)
        }
        (StepResult::Err(left_err), StepResult::Err(right_err)) => {
            if left_err.class != right_err.class {
                return Err(ComparisonMismatch {
                    step: left.step,
                    reason: format!(
                        "error class mismatch: left={:?} right={:?}",
                        left_err.class, right_err.class
                    ),
                });
            }
            if config.compare_error_messages && left_err.message != right_err.message {
                return Err(ComparisonMismatch {
                    step: left.step,
                    reason: format!(
                        "error message mismatch: left={} right={}",
                        left_err.message, right_err.message
                    ),
                });
            }
            Ok(())
        }
        (StepResult::Ok(_), StepResult::Err(err)) => Err(ComparisonMismatch {
            step: left.step,
            reason: format!("left ok, right error: {}", err.message),
        }),
        (StepResult::Err(err), StepResult::Ok(_)) => Err(ComparisonMismatch {
            step: left.step,
            reason: format!("left error, right ok: {}", err.message),
        }),
    }
}

fn compare_observation(
    step: usize,
    left: &ActionObservation,
    right: &ActionObservation,
    config: ComparisonConfig,
) -> Result<(), ComparisonMismatch> {
    match (left, right) {
        (ActionObservation::Simple, ActionObservation::Simple) => Ok(()),
        (ActionObservation::Query(left_query), ActionObservation::Query(right_query)) => {
            compare_query(step, left_query, right_query, config)
        }
        _ => Err(ComparisonMismatch {
            step,
            reason: "observation mismatch".to_string(),
        }),
    }
}

fn compare_query(
    step: usize,
    left: &QueryObservation,
    right: &QueryObservation,
    config: ComparisonConfig,
) -> Result<(), ComparisonMismatch> {
    if config.compare_query_summaries {
        if left.summary != right.summary {
            return Err(ComparisonMismatch {
                step,
                reason: format!(
                    "query summary mismatch: left rows={} cols={} right rows={} cols={}",
                    left.summary.row_count,
                    left.summary.column_count,
                    right.summary.row_count,
                    right.summary.column_count
                ),
            });
        }
    }

    if config.compare_query_values {
        if left.columns != right.columns {
            return Err(ComparisonMismatch {
                step,
                reason: format!("query columns mismatch: left={:?} right={:?}", left.columns, right.columns),
            });
        }
        if left.rows != right.rows {
            return Err(ComparisonMismatch {
                step,
                reason: format!(
                    "query rows mismatch: left_count={} right_count={}",
                    left.rows.len(),
                    right.rows.len()
                ),
            });
        }
    }

    Ok(())
}
