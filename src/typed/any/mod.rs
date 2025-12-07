//! Backend-neutral `AnyIdle` and `AnyTx` enum wrappers.

#[cfg(feature = "postgres")]
use crate::postgres::typed::{Idle as PgIdle, InTx as PgInTx, PgConnection};
#[cfg(feature = "sqlite")]
use crate::sqlite::typed::{Idle as SqIdle, InTx as SqInTx, SqliteTypedConnection};
#[cfg(feature = "turso")]
use crate::turso::typed::{Idle as TuIdle, InTx as TuInTx, TursoConnection};

mod ops;
mod queryable;

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
