mod reporting;

use crate::args::{self, SimConfig};
use crate::backends::build_backend;
use crate::comparator::{ComparisonConfig, ComparisonMismatch, compare_runs};
use crate::plan;
use crate::runner::{PlanRun, RunError, RunSummary};
use crate::shrinker::ShrinkResult;

pub(crate) async fn run_mode(
    config: &SimConfig,
    plan: plan::Plan,
) -> Result<RunSummary, ModeError> {
    if let Some(differential_backend) = config.differential_backend {
        run_differential(config, plan, differential_backend).await
    } else if config.doublecheck {
        run_doublecheck(config, plan).await
    } else {
        run_single(config, plan).await
    }
}

pub(crate) async fn run_with_failure_context(
    config: &SimConfig,
    plan: plan::Plan,
) -> Result<RunSummary, FailureContext> {
    match run_mode(config, plan.clone()).await {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let shrink_result = if config.shrink_on_failure {
                shrink_plan_on_failure(config, plan, &error).await
            } else {
                None
            };
            Err(FailureContext {
                error,
                shrink_result,
            })
        }
    }
}

pub(crate) fn report_failure(config: &SimConfig, plan: &plan::Plan, context: FailureContext) -> ! {
    reporting::report_failure(config, plan, context)
}

#[derive(Debug)]
pub(crate) enum ModeError {
    PlanRun {
        label: &'static str,
        error: Box<RunError>,
    },
    Compare(ComparisonMismatch),
    Setup {
        label: &'static str,
        reason: String,
    },
}

#[derive(Debug)]
pub(crate) struct FailureContext {
    pub(crate) error: ModeError,
    pub(crate) shrink_result: Option<ShrinkResult>,
}

async fn run_single(config: &SimConfig, plan: plan::Plan) -> Result<RunSummary, ModeError> {
    let run = run_once(config, config.backend, plan, "primary").await?;
    if let Some(error) = run.error {
        Err(ModeError::PlanRun {
            label: "primary",
            error: Box::new(error),
        })
    } else {
        Ok(RunSummary {
            steps: run.outcomes.len(),
        })
    }
}

async fn run_doublecheck(config: &SimConfig, plan: plan::Plan) -> Result<RunSummary, ModeError> {
    let first = run_once(config, config.backend, plan.clone(), "doublecheck-1").await?;
    let second = run_once(config, config.backend, plan, "doublecheck-2").await?;

    compare_runs(&first, &second, compare_config(config, true)).map_err(ModeError::Compare)?;
    success_or_plan_error(&first, "doublecheck-1")?;
    success_or_plan_error(&second, "doublecheck-2")?;

    Ok(RunSummary {
        steps: first.outcomes.len(),
    })
}

async fn run_differential(
    config: &SimConfig,
    plan: plan::Plan,
    differential_backend: args::BackendKind,
) -> Result<RunSummary, ModeError> {
    let primary = run_once(config, config.backend, plan.clone(), "primary").await?;
    let secondary = run_once(config, differential_backend, plan, "differential").await?;

    compare_runs(&primary, &secondary, compare_config(config, false))
        .map_err(ModeError::Compare)?;
    success_or_plan_error(&primary, "primary")?;
    success_or_plan_error(&secondary, "differential")?;

    Ok(RunSummary {
        steps: primary.outcomes.len(),
    })
}

async fn run_once(
    config: &SimConfig,
    backend_kind: args::BackendKind,
    plan: plan::Plan,
    label: &'static str,
) -> Result<PlanRun, ModeError> {
    let mut backend = build_backend(backend_kind, config.pool_size, config)
        .await
        .map_err(|err| ModeError::Setup {
            label,
            reason: format!("backend init failed: {err}"),
        })?;

    if !config.reset_tables.is_empty() {
        crate::runner::apply_reset(
            &mut backend,
            backend_kind,
            config.reset_mode,
            &config.reset_tables,
        )
        .await
        .map_err(|err| ModeError::Setup {
            label,
            reason: format!("reset failed: {err}"),
        })?;
    }

    Ok(crate::runner::execute_plan(plan, &mut backend).await)
}

async fn shrink_plan_on_failure(
    config: &SimConfig,
    plan: plan::Plan,
    error: &ModeError,
) -> Option<ShrinkResult> {
    let fingerprint = error_fingerprint(error);
    let shrink_result = crate::shrinker::shrink_plan(plan, config.shrink_max_rounds, |candidate| {
        let candidate = candidate.clone();
        let fingerprint = fingerprint.clone();
        async move {
            match run_mode(config, candidate.clone()).await {
                Ok(_) => false,
                Err(err) => error_fingerprint(&err) == fingerprint,
            }
        }
    })
    .await;
    Some(shrink_result)
}

fn compare_config(config: &SimConfig, compare_error_messages: bool) -> ComparisonConfig {
    let compare_queries = !config.reset_tables.is_empty();
    ComparisonConfig {
        compare_error_messages,
        compare_query_summaries: compare_queries,
        compare_query_values: compare_queries,
    }
}

fn success_or_plan_error(run: &PlanRun, label: &'static str) -> Result<(), ModeError> {
    match &run.error {
        Some(error) => Err(ModeError::PlanRun {
            label,
            error: Box::new(error.clone()),
        }),
        None => Ok(()),
    }
}

fn error_fingerprint(error: &ModeError) -> String {
    match error {
        ModeError::PlanRun { error, .. } => format!("plan_run:{:?}:{}", error.action, error.reason),
        ModeError::Compare(mismatch) => format!("compare:{}", mismatch.reason),
        ModeError::Setup { reason, .. } => format!("setup:{reason}"),
    }
}
