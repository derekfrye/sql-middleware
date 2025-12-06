mod dispatch;
mod targets;

pub use dispatch::{execute_batch, query};
pub(crate) use dispatch::{execute_dml_dispatch, execute_select_dispatch};
pub use targets::{BatchTarget, QueryTarget};
pub(crate) use targets::QueryTargetKind;
