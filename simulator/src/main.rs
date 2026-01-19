mod args;
mod backends;
mod logging;
mod plan;
mod properties;
mod runner;

use clap::Parser;
use tracing::Level;

use crate::args::{Args, SimConfig};
use crate::logging::LogWriter;

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

    if config.plan.is_some() && config.property.is_some() {
        eprintln!("--plan and --property are mutually exclusive");
        std::process::exit(1);
    }

    if let Some(plan_path) = config.plan.clone() {
        let plan = match runner::load_plan(&plan_path) {
            Ok(plan) => plan,
            Err(err) => {
                eprintln!("failed to load plan: {err}");
                std::process::exit(1);
            }
        };
        run_plan(plan, config.pool_size, config.dump_plan_on_failure.as_deref());
        return;
    }

    if let Some(property) = config.property {
        let plan = property.build_plan();
        run_plan(plan, config.pool_size, config.dump_plan_on_failure.as_deref());
        return;
    }

    eprintln!("missing required flag: --plan or --property");
    std::process::exit(1);
}

fn run_plan(plan: plan::Plan, pool_size: usize, dump_path: Option<&std::path::Path>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap_or_else(|err| {
            eprintln!("failed to start async runtime: {err}");
            std::process::exit(1);
        });
    let plan_for_dump = plan.clone();
    match runtime.block_on(runner::run_plan_sqlite(plan, pool_size)) {
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
            eprintln!(
                "plan failed at step {} (task {}): {}",
                err.step, err.task, err.reason
            );
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
