//! Trait implementations for `SQLite` typed connections.

use super::traits::{BeginTx, Queryable, TxConn, TypedConnOps};
use crate::SqlMiddlewareDbError;
use crate::sqlite::typed::{Idle as SqIdle, InTx as SqInTx, SqliteTypedConnection};
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

impl Queryable for SqliteTypedConnection<SqIdle> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

impl Queryable for SqliteTypedConnection<SqInTx> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

impl TypedConnOps for SqliteTypedConnection<SqIdle> {
    #[allow(clippy::manual_async_fn)]
    fn execute_batch(
        &mut self,
        sql: &str,
    ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>> {
        async move { self.execute_batch(sql).await }
    }

    #[allow(clippy::manual_async_fn)]
    fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>> {
        async move { self.dml(query, params).await }
    }

    #[allow(clippy::manual_async_fn)]
    fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>> {
        async move { self.select(query, params).await }
    }
}

impl TypedConnOps for SqliteTypedConnection<SqInTx> {
    #[allow(clippy::manual_async_fn)]
    fn execute_batch(
        &mut self,
        sql: &str,
    ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>> {
        async move { self.execute_batch(sql).await }
    }

    #[allow(clippy::manual_async_fn)]
    fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>> {
        async move { self.dml(query, params).await }
    }

    #[allow(clippy::manual_async_fn)]
    fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>> {
        async move { self.select(query, params).await }
    }
}

impl BeginTx for SqliteTypedConnection<SqIdle> {
    type Tx = SqliteTypedConnection<SqInTx>;

    #[allow(clippy::manual_async_fn)]
    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move { self.begin().await }
    }
}

impl TxConn for SqliteTypedConnection<SqInTx> {
    type Idle = SqliteTypedConnection<SqIdle>;

    #[allow(clippy::manual_async_fn)]
    fn commit(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move { self.commit().await }
    }

    #[allow(clippy::manual_async_fn)]
    fn rollback(
        self,
    ) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move { self.rollback().await }
    }
}
