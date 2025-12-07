//! Trait implementations for Turso typed connections.

use super::traits::{BeginTx, Queryable, TxConn, TypedConnOps};
use crate::SqlMiddlewareDbError;
use crate::turso::typed::{Idle as TuIdle, InTx as TuInTx, TursoConnection};
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

impl Queryable for TursoConnection<TuIdle> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

impl Queryable for TursoConnection<TuInTx> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

impl TypedConnOps for TursoConnection<TuIdle> {
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

impl TypedConnOps for TursoConnection<TuInTx> {
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

impl BeginTx for TursoConnection<TuIdle> {
    type Tx = TursoConnection<TuInTx>;

    #[allow(clippy::manual_async_fn)]
    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move { self.begin().await }
    }
}

impl TxConn for TursoConnection<TuInTx> {
    type Idle = TursoConnection<TuIdle>;

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
