//! Trait implementations for `SQLite` typed connections.

use super::macros::impl_typed_backend;
use super::traits::{BeginTx, Queryable, TxConn, TypedConnOps};
use crate::SqlMiddlewareDbError;
use crate::sqlite::typed::{Idle as SqIdle, InTx as SqInTx, SqliteTypedConnection};
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

impl_typed_backend!(SqliteTypedConnection, SqIdle, SqInTx);
