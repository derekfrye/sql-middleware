mod args;
mod backend;
mod driver;
mod logging;
mod model;
mod oracle;
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

    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
    run(config, &mut rng);
}
