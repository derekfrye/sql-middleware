use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;

use crate::properties::PropertyKind;

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
pub(crate) enum ResetMode {
    Truncate,
    Delete,
    Recreate,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
pub(crate) enum BackendKind {
    Sqlite,
    Postgres,
    Turso,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Deterministic sql-middleware simulator")]
pub(crate) struct Args {
    #[arg(long, value_enum, default_value = "sqlite")]
    pub(crate) backend: BackendKind,
    #[arg(long)]
    pub(crate) doublecheck: bool,
    #[arg(long, value_enum)]
    pub(crate) differential_backend: Option<BackendKind>,
    #[arg(long)]
    pub(crate) plan: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub(crate) property: Option<PropertyKind>,
    #[arg(long)]
    pub(crate) generate: bool,
    #[arg(long)]
    pub(crate) steps: Option<usize>,
    #[arg(long)]
    pub(crate) seed: Option<u64>,
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
    #[arg(long, default_value_t = 8)]
    pub(crate) pool_size: usize,
    #[arg(long)]
    pub(crate) log: Option<PathBuf>,
    #[arg(long)]
    pub(crate) dump_plan_on_failure: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "truncate")]
    pub(crate) reset_mode: ResetMode,
    #[arg(long, value_name = "TABLE")]
    pub(crate) reset_table: Vec<String>,
    #[arg(long)]
    pub(crate) pg_dbname: Option<String>,
    #[arg(long)]
    pub(crate) pg_host: Option<String>,
    #[arg(long)]
    pub(crate) pg_port: Option<u16>,
    #[arg(long)]
    pub(crate) pg_user: Option<String>,
    #[arg(long)]
    pub(crate) pg_password: Option<String>,
    #[arg(long, default_value = "/tmp/sql-middleware-simulator-turso.db")]
    pub(crate) turso_db_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SimConfig {
    pub(crate) backend: BackendKind,
    pub(crate) doublecheck: bool,
    pub(crate) differential_backend: Option<BackendKind>,
    pub(crate) plan: Option<PathBuf>,
    pub(crate) property: Option<PropertyKind>,
    pub(crate) generate: bool,
    pub(crate) steps: usize,
    pub(crate) seed: u64,
    pub(crate) tasks: usize,
    pub(crate) ddl_rate: f64,
    pub(crate) busy_rate: f64,
    pub(crate) panic_rate: f64,
    pub(crate) sleep_rate: f64,
    pub(crate) max_in_flight_tx: usize,
    pub(crate) pool_size: usize,
    pub(crate) log: Option<PathBuf>,
    pub(crate) dump_plan_on_failure: Option<PathBuf>,
    pub(crate) reset_mode: ResetMode,
    pub(crate) reset_tables: Vec<String>,
    pub(crate) pg_dbname: Option<String>,
    pub(crate) pg_host: Option<String>,
    pub(crate) pg_port: Option<u16>,
    pub(crate) pg_user: Option<String>,
    pub(crate) pg_password: Option<String>,
    pub(crate) turso_db_path: String,
}

impl SimConfig {
    pub(crate) fn from_args(args: Args) -> Self {
        SimConfig {
            backend: args.backend,
            doublecheck: args.doublecheck,
            differential_backend: args.differential_backend,
            plan: args.plan,
            property: args.property,
            generate: args.generate,
            steps: args.steps.unwrap_or(1_000),
            seed: args.seed.unwrap_or_else(random_seed),
            tasks: args.tasks.max(1),
            ddl_rate: clamp_rate(args.ddl_rate),
            busy_rate: clamp_rate(args.busy_rate),
            panic_rate: clamp_rate(args.panic_rate),
            sleep_rate: clamp_rate(args.sleep_rate),
            max_in_flight_tx: args.max_in_flight_tx.max(1),
            pool_size: args.pool_size,
            log: args.log,
            dump_plan_on_failure: args.dump_plan_on_failure,
            reset_mode: args.reset_mode,
            reset_tables: args.reset_table,
            pg_dbname: args.pg_dbname,
            pg_host: args.pg_host,
            pg_port: args.pg_port,
            pg_user: args.pg_user,
            pg_password: args.pg_password,
            turso_db_path: args.turso_db_path,
        }
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
