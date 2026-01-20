mod args;
mod backends;
mod comparator;
mod generation;
mod logging;
mod plan;
mod properties;
mod runner;

use clap::Parser;
use tracing::Level;

use crate::args::{Args, SimConfig};
use crate::backends::build_backend;
use crate::comparator::{compare_runs, ComparisonConfig, ComparisonMismatch};
use crate::logging::LogWriter;
use crate::runner::{PlanRun, RunError, RunSummary};

fn main() {
    let args = Args::parse();
    let config = SimConfig::from_args(args);
    let writer = LogWriter::new(config.log.clone()).unwrap_or_else(|err| {
        eprintln!("failed to open log file: {err}");
        std::process::exit(1);
    });

    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_target(false)
        .with_max_level(Level::INFO)
        .init();

    let config_json = serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".to_string());
    tracing::info!("config: {}", config_json);

    if config.plan.is_some() && config.generate {
        eprintln!("--plan and --generate are mutually exclusive");
        std::process::exit(1);
    }
    if config.plan.is_some() && config.property.is_some() {
        eprintln!("--plan and --property are mutually exclusive");
        std::process::exit(1);
    }
    if config.doublecheck && config.differential_backend.is_some() {
        eprintln!("--doublecheck and --differential-backend are mutually exclusive");
        std::process::exit(1);
    }
    if let Some(differential_backend) = config.differential_backend {
        if differential_backend == config.backend {
            eprintln!("--differential-backend must differ from --backend");
            std::process::exit(1);
        }
    }

    if config.generate {
        match generation::generate_plan(&config) {
            Ok(plan) => run_plan(&config, plan, config.dump_plan_on_failure.as_deref()),
            Err(err) => {
                eprintln!("failed to generate plan: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(plan_path) = config.plan.clone() {
        let plan = match runner::load_plan(&plan_path) {
            Ok(plan) => plan,
            Err(err) => {
                eprintln!("failed to load plan: {err}");
                std::process::exit(1);
            }
        };
        run_plan(&config, plan, config.dump_plan_on_failure.as_deref());
        return;
    }

    if let Some(property) = config.property {
        let plan = property.build_plan();
        run_plan(&config, plan, config.dump_plan_on_failure.as_deref());
        return;
    }

    eprintln!("missing required flag: --plan, --property, or --generate");
    std::process::exit(1);
}

fn run_plan(config: &SimConfig, plan: plan::Plan, dump_path: Option<&std::path::Path>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap_or_else(|err| {
            eprintln!("failed to start async runtime: {err}");
            std::process::exit(1);
        });
    let plan_for_dump = plan.clone();
    match runtime.block_on(run_mode(config, plan)) {
        Ok(summary) => {
            tracing::info!("plan complete: steps={}", summary.steps);
        }
        Err(err) => {
            if let Some(path) = dump_path {
                if let Err(dump_err) = dump_plan(path, &plan_for_dump) {
                    eprintln!("failed to dump plan to {}: {dump_err}", path.display());
                } else {
                    eprintln!(
                        "dumped failing plan to {} (replay with --plan {})",
                        path.display(),
                        path.display()
                    );
                }
            }
            report_mode_error(err);
            std::process::exit(1);
        }
    }
}

fn dump_plan(path: &std::path::Path, plan: &plan::Plan) -> Result<(), String> {
    let content = serde_json::to_string_pretty(plan)
        .map_err(|err| format!("failed to serialize plan: {err}"))?;
    std::fs::write(path, content)
        .map_err(|err| format!("failed to write plan file: {err}"))?;
    Ok(())
}

async fn run_mode(config: &SimConfig, plan: plan::Plan) -> Result<RunSummary, ModeError> {
    if let Some(differential_backend) = config.differential_backend {
        run_differential(config, plan, differential_backend).await
    } else if config.doublecheck {
        run_doublecheck(config, plan).await
    } else {
        run_single(config, plan).await
    }
}

async fn run_single(config: &SimConfig, plan: plan::Plan) -> Result<RunSummary, ModeError> {
    let run = run_once(config, config.backend, plan, "primary").await?;
    if let Some(err) = run.error {
        return Err(ModeError::PlanRun {
            label: "primary",
            error: err,
        });
    }
    Ok(RunSummary {
        steps: run.outcomes.len(),
    })
}

async fn run_doublecheck(config: &SimConfig, plan: plan::Plan) -> Result<RunSummary, ModeError> {
    let first = run_once(config, config.backend, plan.clone(), "doublecheck-1").await?;
    let second = run_once(config, config.backend, plan, "doublecheck-2").await?;

    let compare_queries = !config.reset_tables.is_empty();
    let compare_config = ComparisonConfig {
        compare_error_messages: true,
        compare_query_summaries: compare_queries,
        compare_query_values: compare_queries,
    };
    compare_runs(&first, &second, compare_config)
        .map_err(ModeError::Compare)?;

    if let Some(err) = first.error {
        return Err(ModeError::PlanRun {
            label: "doublecheck-1",
            error: err,
        });
    }
    if let Some(err) = second.error {
        return Err(ModeError::PlanRun {
            label: "doublecheck-2",
            error: err,
        });
    }

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
    let secondary = run_once(
        config,
        differential_backend,
        plan,
        "differential",
    )
    .await?;

    let compare_queries = !config.reset_tables.is_empty();
    let compare_config = ComparisonConfig {
        compare_error_messages: false,
        compare_query_summaries: compare_queries,
        compare_query_values: compare_queries,
    };
    compare_runs(&primary, &secondary, compare_config)
        .map_err(ModeError::Compare)?;

    if let Some(err) = primary.error {
        return Err(ModeError::PlanRun {
            label: "primary",
            error: err,
        });
    }
    if let Some(err) = secondary.error {
        return Err(ModeError::PlanRun {
            label: "differential",
            error: err,
        });
    }

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
        runner::apply_reset(
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
    Ok(runner::execute_plan(plan, &mut backend).await)
}

#[derive(Debug)]
enum ModeError {
    PlanRun { label: &'static str, error: RunError },
    Compare(ComparisonMismatch),
    Setup { label: &'static str, reason: String },
}

fn report_mode_error(err: ModeError) {
    match err {
        ModeError::PlanRun { label, error } => {
            eprintln!(
                "{label} run failed at step {} (task {}): {}",
                error.step, error.task, error.reason
            );
        }
        ModeError::Compare(mismatch) => {
            eprintln!(
                "comparison failed at step {}: {}",
                mismatch.step, mismatch.reason
            );
        }
        ModeError::Setup { label, reason } => {
            eprintln!("{label} setup failed: {reason}");
        }
    }
}
