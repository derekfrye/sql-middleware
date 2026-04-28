mod app;
mod args;
mod backends;
mod bugbase;
mod comparator;
mod generation;
mod logging;
mod plan;
mod properties;
mod runner;
mod shrinker;

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
    if let Some(differential_backend) = config.differential_backend
        && differential_backend == config.backend
    {
        eprintln!("--differential-backend must differ from --backend");
        std::process::exit(1);
    }

    if config.generate {
        match generation::generate_plan(&config) {
            Ok(plan) => run_plan(&config, plan),
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
        run_plan(&config, plan);
        return;
    }

    if let Some(property) = config.property {
        let plan = property.build_plan();
        run_plan(&config, plan);
        return;
    }

    eprintln!("missing required flag: --plan, --property, or --generate");
    std::process::exit(1);
}

fn run_plan(config: &SimConfig, plan: plan::Plan) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap_or_else(|err| {
            eprintln!("failed to start async runtime: {err}");
            std::process::exit(1);
        });
    let plan_for_dump = plan.clone();
    match runtime.block_on(app::run_with_failure_context(config, plan)) {
        Ok(summary) => {
            tracing::info!("plan complete: steps={}", summary.steps);
        }
        Err(context) => {
            app::report_failure(config, &plan_for_dump, context);
        }
    }
}
