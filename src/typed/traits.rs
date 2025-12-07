//! Core traits for typed database connections.

use crate::SqlMiddlewareDbError;
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

/// Minimal query surface shared by idle and tx connections.
pub trait Queryable {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a>;
}

/// Shared operations available in both idle and transactional typed connections.
pub trait TypedConnOps: Queryable {
    #[allow(clippy::manual_async_fn)]
    fn execute_batch(
        &mut self,
        sql: &str,
    ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>>;

    #[allow(clippy::manual_async_fn)]
    fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>>;

    #[allow(clippy::manual_async_fn)]
    fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>>;
}

/// Begin a transaction from an idle connection.
pub trait BeginTx: Sized {
    type Tx: TxConn<Idle = Self>;

    #[allow(clippy::manual_async_fn)]
    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>>;
}

/// Transaction state that can return to idle.
pub trait TxConn {
    type Idle;

    #[allow(clippy::manual_async_fn)]
    fn commit(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>>;
    #[allow(clippy::manual_async_fn)]
    fn rollback(
        self,
    ) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>>;
}
