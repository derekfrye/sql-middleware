mod args;
mod backend;
mod backends;
mod driver;
mod logging;
mod model;
mod oracle;
mod plan;
mod runner;
mod scheduler;

use clap::Parser;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tracing::Level;

use crate::args::{Args, SimConfig};
use crate::driver::run;
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

    if let Some(plan_path) = config.plan.clone() {
        let plan = match runner::load_plan(&plan_path) {
            Ok(plan) => plan,
            Err(err) => {
                eprintln!("failed to load plan: {err}");
                std::process::exit(1);
            }
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap_or_else(|err| {
                eprintln!("failed to start async runtime: {err}");
                std::process::exit(1);
            });
        match runtime.block_on(runner::run_plan_sqlite(plan, config.pool_size)) {
            Ok(summary) => {
                tracing::info!("plan complete: steps={}", summary.steps);
            }
            Err(err) => {
                eprintln!(
                    "plan failed at step {} (task {}): {}",
                    err.step, err.task, err.reason
                );
                std::process::exit(1);
            }
        }
        return;
    }

    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
    run(config, &mut rng);
}
