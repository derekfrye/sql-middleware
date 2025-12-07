mod dispatch;
mod targets;

pub use dispatch::{execute_batch, query};
pub(crate) use dispatch::{execute_dml_dispatch, execute_select_dispatch};
pub(crate) use targets::QueryTargetKind;
pub use targets::{BatchTarget, QueryTarget};
