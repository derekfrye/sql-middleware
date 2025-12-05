//! Backend-agnostic typestate traits and enums for typed connections.
//!
//! For now only Postgres implements these; other backends can hook in by
//! implementing the traits and adding enum variants.

use crate::query_builder::QueryBuilder;
use crate::SqlMiddlewareDbError;

#[cfg(feature = "typed-postgres")]
use crate::typed_postgres::{Idle as PgIdle, InTx as PgInTx, PgConnection};

/// Minimal query surface shared by idle and tx connections.
pub trait Queryable {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a>;
}

/// Begin a transaction from an idle connection.
pub trait BeginTx: Sized {
    type Tx: TxConn<Idle = Self>;

    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>>;
}

/// Transaction state that can return to idle.
pub trait TxConn {
    type Idle;

    fn commit(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>>;
    fn rollback(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>>;
}

/// Backend-neutral idle wrapper.
pub enum AnyIdle {
    #[cfg(feature = "typed-postgres")]
    Postgres(PgConnection<PgIdle>),
}

/// Backend-neutral tx wrapper.
pub enum AnyTx {
    #[cfg(feature = "typed-postgres")]
    Postgres(PgConnection<PgInTx>),
}

impl Queryable for AnyIdle {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "typed-postgres")]
            AnyIdle::Postgres(conn) => conn.query(sql),
        }
    }
}

impl Queryable for AnyTx {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "typed-postgres")]
            AnyTx::Postgres(conn) => conn.query(sql),
        }
    }
}

impl BeginTx for AnyIdle {
    type Tx = AnyTx;

    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "typed-postgres")]
                AnyIdle::Postgres(conn) => Ok(AnyTx::Postgres(conn.begin().await?)),
            }
        }
    }
}

impl TxConn for AnyTx {
    type Idle = AnyIdle;

    fn commit(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "typed-postgres")]
                AnyTx::Postgres(tx) => Ok(AnyIdle::Postgres(tx.commit().await?)),
            }
        }
    }

    fn rollback(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move {
            match self {
                #[cfg(feature = "typed-postgres")]
                AnyTx::Postgres(tx) => Ok(AnyIdle::Postgres(tx.rollback().await?)),
            }
        }
    }
}

// Implement traits for the concrete Postgres types so helpers can take &mut impl Queryable directly.
#[cfg(feature = "typed-postgres")]
impl Queryable for PgConnection<PgIdle> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

#[cfg(feature = "typed-postgres")]
impl Queryable for PgConnection<PgInTx> {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        self.query(sql)
    }
}

#[cfg(feature = "typed-postgres")]
impl BeginTx for PgConnection<PgIdle> {
    type Tx = PgConnection<PgInTx>;

    fn begin(self) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
        async move { self.begin().await }
    }
}

#[cfg(feature = "typed-postgres")]
impl TxConn for PgConnection<PgInTx> {
    type Idle = PgConnection<PgIdle>;

    fn commit(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move { self.commit().await }
    }

    fn rollback(self) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
        async move { self.rollback().await }
    }
}
