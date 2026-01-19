use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;

use crate::properties::PropertyKind;

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
pub(crate) enum BackendKind {
    Sqlite,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Deterministic sql-middleware simulator")]
pub(crate) struct Args {
    #[arg(long, value_enum, default_value = "sqlite")]
    pub(crate) backend: BackendKind,
    #[arg(long)]
    pub(crate) plan: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub(crate) property: Option<PropertyKind>,
    #[arg(long, default_value_t = 8)]
    pub(crate) pool_size: usize,
    #[arg(long)]
    pub(crate) log: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SimConfig {
    pub(crate) backend: BackendKind,
    pub(crate) plan: Option<PathBuf>,
    pub(crate) property: Option<PropertyKind>,
    pub(crate) pool_size: usize,
    pub(crate) log: Option<PathBuf>,
}

impl SimConfig {
    pub(crate) fn from_args(args: Args) -> Self {
        SimConfig {
            backend: args.backend,
            plan: args.plan,
            property: args.property,
            pool_size: args.pool_size,
            log: args.log,
        }
    }
}
