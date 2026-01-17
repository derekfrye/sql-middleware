//! Trait implementations for Postgres typed connections.

use super::macros::impl_typed_backend;
use super::traits::{BeginTx, Queryable, TxConn, TypedConnOps};
use crate::SqlMiddlewareDbError;
use crate::postgres::typed::{Idle as PgIdle, InTx as PgInTx, PgConnection};
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

impl_typed_backend!(PgConnection, PgIdle, PgInTx);
