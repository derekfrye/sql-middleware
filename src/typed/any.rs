//! Backend-neutral `AnyIdle` and `AnyTx` enum wrappers.

use super::traits::{BeginTx, Queryable, TxConn, TypedConnOps};
use crate::SqlMiddlewareDbError;
use crate::{middleware::RowValues, query_builder::QueryBuilder, results::ResultSet};

#[cfg(feature = "postgres")]
use crate::postgres::typed::{Idle as PgIdle, InTx as PgInTx, PgConnection};
#[cfg(feature = "sqlite")]
use crate::sqlite::typed::{Idle as SqIdle, InTx as SqInTx, SqliteTypedConnection};
#[cfg(feature = "turso")]
use crate::turso::typed::{Idle as TuIdle, InTx as TuInTx, TursoConnection};

/// Backend-neutral idle wrapper.
pub enum AnyIdle {
    #[cfg(feature = "postgres")]
    Postgres(PgConnection<PgIdle>),
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteTypedConnection<SqIdle>),
    #[cfg(feature = "turso")]
    Turso(TursoConnection<TuIdle>),
}

/// Backend-neutral tx wrapper.
pub enum AnyTx {
    #[cfg(feature = "postgres")]
    Postgres(PgConnection<PgInTx>),
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteTypedConnection<SqInTx>),
    #[cfg(feature = "turso")]
    Turso(TursoConnection<TuInTx>),
}

impl Queryable for AnyIdle {
    fn query<'a>(&'a mut self, _sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "postgres")]
            AnyIdle::Postgres(conn) => conn.query(_sql),
            #[cfg(feature = "sqlite")]
            AnyIdle::Sqlite(conn) => conn.query(_sql),
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
            #[cfg(feature = "sqlite")]
            AnyTx::Sqlite(conn) => conn.query(_sql),
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
                #[cfg(feature = "sqlite")]
                AnyIdle::Sqlite(conn) => conn.execute_batch(sql).await,
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
                #[cfg(feature = "sqlite")]
                AnyIdle::Sqlite(conn) => conn.dml(query, params).await,
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
                #[cfg(feature = "sqlite")]
                AnyIdle::Sqlite(conn) => conn.select(query, params).await,
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
                #[cfg(feature = "sqlite")]
                AnyTx::Sqlite(conn) => conn.execute_batch(sql).await,
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
                #[cfg(feature = "sqlite")]
                AnyTx::Sqlite(conn) => conn.dml(query, params).await,
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
                #[cfg(feature = "sqlite")]
                AnyTx::Sqlite(conn) => conn.select(query, params).await,
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
                #[cfg(feature = "sqlite")]
                AnyIdle::Sqlite(conn) => Ok(AnyTx::Sqlite(conn.begin().await?)),
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
                #[cfg(feature = "sqlite")]
                AnyTx::Sqlite(tx) => Ok(AnyIdle::Sqlite(tx.commit().await?)),
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
                #[cfg(feature = "sqlite")]
                AnyTx::Sqlite(tx) => Ok(AnyIdle::Sqlite(tx.rollback().await?)),
                #[cfg(feature = "turso")]
                AnyTx::Turso(tx) => Ok(AnyIdle::Turso(tx.rollback().await?)),
                #[allow(unreachable_patterns)]
                _ => unreachable!("typed backends are not enabled"),
            }
        }
    }
}
