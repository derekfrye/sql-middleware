//! Backend-agnostic typestate traits and enums for typed connections.
//!
//! For now only Postgres implements these; other backends can hook in by
//! implementing the traits and adding enum variants.

use crate::SqlMiddlewareDbError;
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

#[cfg(feature = "postgres")]
use crate::postgres::typed::{Idle as PgIdle, InTx as PgInTx, PgConnection};
#[cfg(feature = "turso")]
use crate::turso::typed::{Idle as TuIdle, InTx as TuInTx, TursoConnection};

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

/// Backend-neutral idle wrapper.
pub enum AnyIdle {
    #[cfg(feature = "postgres")]
    Postgres(PgConnection<PgIdle>),
    #[cfg(feature = "turso")]
    Turso(TursoConnection<TuIdle>),
}

/// Backend-neutral tx wrapper.
pub enum AnyTx {
    #[cfg(feature = "postgres")]
    Postgres(PgConnection<PgInTx>),
    #[cfg(feature = "turso")]
    Turso(TursoConnection<TuInTx>),
}

impl Queryable for AnyIdle {
    fn query<'a>(&'a mut self, _sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "postgres")]
            AnyIdle::Postgres(conn) => conn.query(_sql),
            #[cfg(feature = "turso")]
            AnyIdle::Turso(conn) => conn.query(_sql),
            #[allow(unreachable_patterns)]
            _ => unreachable!("typed backends are not enabled"),
        }
    }
}

impl Queryable for AnyTx {
    fn query<'a>(&'a mut self, _sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "postgres")]
            AnyTx::Postgres(conn) => conn.query(_sql),
            #[cfg(feature = "turso")]
            AnyTx::Turso(conn) => conn.query(_sql),
            #[allow(unreachable_patterns)]
            _ => unreachable!("typed backends are not enabled"),
        }
    }
}

impl TypedConnOps for AnyIdle {
    #[allow(clippy::manual_async_fn)]
    fn execute_batch(
        &mut self,
        sql: &str,
    ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyIdle::Postgres(conn) => conn.execute_batch(sql).await,
                #[cfg(feature = "turso")]
                AnyIdle::Turso(conn) => conn.execute_batch(sql).await,
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyIdle::Postgres(conn) => conn.dml(query, params).await,
                #[cfg(feature = "turso")]
                AnyIdle::Turso(conn) => conn.dml(query, params).await,
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyIdle::Postgres(conn) => conn.select(query, params).await,
                #[cfg(feature = "turso")]
                AnyIdle::Turso(conn) => conn.select(query, params).await,
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }
}

impl TypedConnOps for AnyTx {
    #[allow(clippy::manual_async_fn)]
    fn execute_batch(
        &mut self,
        sql: &str,
    ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyTx::Postgres(conn) => conn.execute_batch(sql).await,
                #[cfg(feature = "turso")]
                AnyTx::Turso(conn) => conn.execute_batch(sql).await,
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyTx::Postgres(conn) => conn.dml(query, params).await,
                #[cfg(feature = "turso")]
                AnyTx::Turso(conn) => conn.dml(query, params).await,
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyTx::Postgres(conn) => conn.select(query, params).await,
                #[cfg(feature = "turso")]
                AnyTx::Turso(conn) => conn.select(query, params).await,
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }
}

impl BeginTx for AnyIdle {
    type Tx = AnyTx;

    #[allow(clippy::manual_async_fn)]
    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyIdle::Postgres(conn) => Ok(AnyTx::Postgres(conn.begin().await?)),
                #[cfg(feature = "turso")]
                AnyIdle::Turso(conn) => Ok(AnyTx::Turso(conn.begin().await?)),
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }
}

impl TxConn for AnyTx {
    type Idle = AnyIdle;

    #[allow(clippy::manual_async_fn)]
    fn commit(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyTx::Postgres(tx) => Ok(AnyIdle::Postgres(tx.commit().await?)),
                #[cfg(feature = "turso")]
                AnyTx::Turso(tx) => Ok(AnyIdle::Turso(tx.commit().await?)),
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn rollback(
        self,
    ) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "postgres")]
                AnyTx::Postgres(tx) => Ok(AnyIdle::Postgres(tx.rollback().await?)),
                #[cfg(feature = "turso")]
                AnyTx::Turso(tx) => Ok(AnyIdle::Turso(tx.rollback().await?)),
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }
}

// Implement traits for the concrete Postgres types so helpers can take &mut impl Queryable directly.
#[cfg(feature = "postgres")]
impl Queryable for PgConnection<PgIdle> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

#[cfg(feature = "postgres")]
impl Queryable for PgConnection<PgInTx> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

#[cfg(feature = "postgres")]
impl TypedConnOps for PgConnection<PgIdle> {
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

#[cfg(feature = "postgres")]
impl TypedConnOps for PgConnection<PgInTx> {
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

#[cfg(feature = "postgres")]
impl BeginTx for PgConnection<PgIdle> {
    type Tx = PgConnection<PgInTx>;

    #[allow(clippy::manual_async_fn)]
    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move { self.begin().await }
    }
}

#[cfg(feature = "postgres")]
impl TxConn for PgConnection<PgInTx> {
    type Idle = PgConnection<PgIdle>;

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

// Turso impls
#[cfg(feature = "turso")]
impl Queryable for TursoConnection<TuIdle> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

#[cfg(feature = "turso")]
impl Queryable for TursoConnection<TuInTx> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

#[cfg(feature = "turso")]
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

#[cfg(feature = "turso")]
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

#[cfg(feature = "turso")]
impl BeginTx for TursoConnection<TuIdle> {
    type Tx = TursoConnection<TuInTx>;

    #[allow(clippy::manual_async_fn)]
    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move { self.begin().await }
    }
}

#[cfg(feature = "turso")]
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
