use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
pub(crate) enum BackendKind {
    Sqlite,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Deterministic sql-middleware simulator")]
pub(crate) struct Args {
    #[arg(long, value_enum, default_value = "sqlite")]
    pub(crate) backend: BackendKind,
    #[arg(long, value_parser = humantime::parse_duration)]
    pub(crate) duration: Option<Duration>,
    #[arg(long)]
    pub(crate) iterations: Option<u64>,
    #[arg(long)]
    pub(crate) seed: Option<u64>,
    #[arg(long, default_value_t = 8)]
    pub(crate) pool_size: usize,
    #[arg(long, default_value_t = 16)]
    pub(crate) tasks: usize,
    #[arg(long, default_value_t = 0.02)]
    pub(crate) ddl_rate: f64,
    #[arg(long, default_value_t = 0.01)]
    pub(crate) busy_rate: f64,
    #[arg(long, default_value_t = 0.001)]
    pub(crate) panic_rate: f64,
    #[arg(long, default_value_t = 0.05)]
    pub(crate) sleep_rate: f64,
    #[arg(long, default_value_t = 4)]
    pub(crate) max_in_flight_tx: usize,
    #[arg(long)]
    pub(crate) log: Option<PathBuf>,
    #[arg(long)]
    pub(crate) quick: bool,
    #[arg(long)]
    pub(crate) stress: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SimConfig {
    pub(crate) backend: BackendKind,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) iterations: Option<u64>,
    pub(crate) seed: u64,
    pub(crate) pool_size: usize,
    pub(crate) tasks: usize,
    pub(crate) ddl_rate: f64,
    pub(crate) busy_rate: f64,
    pub(crate) panic_rate: f64,
    pub(crate) sleep_rate: f64,
    pub(crate) max_in_flight_tx: usize,
    pub(crate) log: Option<PathBuf>,
    pub(crate) preset: Option<String>,
    pub(crate) first_steps: usize,
    pub(crate) tail_steps: usize,
}

impl SimConfig {
    pub(crate) fn from_args(args: Args) -> Self {
        let mut config = SimConfig {
            backend: args.backend,
            duration_ms: args.duration.map(|d| d.as_millis() as u64),
            iterations: args.iterations,
            seed: args.seed.unwrap_or_else(random_seed),
            pool_size: args.pool_size,
            tasks: args.tasks,
            ddl_rate: clamp_rate(args.ddl_rate),
            busy_rate: clamp_rate(args.busy_rate),
            panic_rate: clamp_rate(args.panic_rate),
            sleep_rate: clamp_rate(args.sleep_rate),
            max_in_flight_tx: args.max_in_flight_tx.max(1),
            log: args.log,
            preset: None,
            first_steps: 30,
            tail_steps: 80,
        };

        if args.quick {
            config.apply_quick();
        }
        if args.stress {
            config.apply_stress();
        }

        config
    }

    fn apply_quick(&mut self) {
        self.preset = Some("quick".to_string());
        self.iterations = Some(10_000);
        self.duration_ms = None;
        self.pool_size = 4;
        self.tasks = 4;
        self.ddl_rate = 0.01;
        self.busy_rate = 0.01;
        self.panic_rate = 0.0005;
        self.sleep_rate = 0.05;
        self.max_in_flight_tx = 2;
    }

    fn apply_stress(&mut self) {
        self.preset = Some("stress".to_string());
        self.iterations = Some(250_000);
        self.duration_ms = None;
        self.pool_size = 16;
        self.tasks = 64;
        self.ddl_rate = 0.05;
        self.busy_rate = 0.03;
        self.panic_rate = 0.002;
        self.sleep_rate = 0.08;
        self.max_in_flight_tx = 8;
    }
}

fn clamp_rate(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    now.as_secs() ^ (now.subsec_nanos() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_rate_limits_bounds() {
        assert_eq!(clamp_rate(-1.0), 0.0);
        assert_eq!(clamp_rate(2.0), 1.0);
        assert_eq!(clamp_rate(0.5), 0.5);
    }
}
