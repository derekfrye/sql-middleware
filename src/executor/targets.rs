use crate::pool::MiddlewarePoolConnection;
use crate::types::StatementCacheMode;

mod typed;

#[cfg(feature = "mssql")]
use crate::mssql;
#[cfg(feature = "mssql")]
use crate::mssql::typed::MssqlManager;
#[cfg(feature = "postgres")]
use crate::postgres;
#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;
#[cfg(feature = "sqlite")]
use crate::sqlite::config::SqliteManager;
#[cfg(feature = "turso")]
use crate::turso;
#[cfg(feature = "turso")]
use crate::typed_turso::TursoManager;
#[cfg(any(
    feature = "postgres",
    feature = "turso",
    feature = "sqlite",
    feature = "mssql"
))]
use bb8::PooledConnection;

/// Target for batch execution (connection or transaction).
pub enum BatchTarget<'a> {
    Connection(&'a mut MiddlewarePoolConnection),
    #[cfg(feature = "postgres")]
    PostgresTx(&'a postgres::transaction::Tx<'a>),
    #[cfg(feature = "mssql")]
    MssqlTx(&'a mut mssql::transaction::Tx<'a>),
    #[cfg(feature = "turso")]
    TursoTx(&'a turso::transaction::Tx<'a>),
    #[cfg(feature = "turso")]
    TypedTurso {
        conn: &'a mut PooledConnection<'static, TursoManager>,
    },
    #[cfg(feature = "turso")]
    TypedTursoTx {
        conn: &'a mut PooledConnection<'static, TursoManager>,
    },
}

/// Target for query builder dispatch (connection or transaction) with a translation default.
pub struct QueryTarget<'a> {
    pub(crate) kind: QueryTargetKind<'a>,
    pub(crate) translation_default: bool,
    pub(crate) statement_cache_mode: StatementCacheMode,
}

pub(crate) enum QueryTargetKind<'a> {
    Connection(&'a mut MiddlewarePoolConnection),
    #[cfg(feature = "postgres")]
    PostgresTx(&'a postgres::transaction::Tx<'a>),
    #[cfg(feature = "sqlite")]
    TypedSqlite {
        conn: &'a mut PooledConnection<'static, SqliteManager>,
    },
    #[cfg(feature = "sqlite")]
    TypedSqliteTx {
        conn: &'a mut PooledConnection<'static, SqliteManager>,
    },
    #[cfg(feature = "postgres")]
    TypedPostgres {
        conn: &'a mut PooledConnection<'static, PgManager>,
    },
    #[cfg(feature = "postgres")]
    TypedPostgresTx {
        conn: &'a mut PooledConnection<'static, PgManager>,
    },
    #[cfg(feature = "mssql")]
    TypedMssql {
        conn: &'a mut PooledConnection<'static, MssqlManager>,
    },
    #[cfg(feature = "mssql")]
    TypedMssqlTx {
        conn: &'a mut PooledConnection<'static, MssqlManager>,
    },
    #[cfg(feature = "turso")]
    TypedTurso {
        conn: &'a mut PooledConnection<'static, TursoManager>,
    },
    #[cfg(feature = "turso")]
    TypedTursoTx {
        conn: &'a mut PooledConnection<'static, TursoManager>,
    },
    #[cfg(feature = "mssql")]
    MssqlTx(&'a mut mssql::transaction::Tx<'a>),
    #[cfg(feature = "turso")]
    TursoTx(&'a turso::transaction::Tx<'a>),
}

impl<'a> From<&'a mut MiddlewarePoolConnection> for BatchTarget<'a> {
    fn from(conn: &'a mut MiddlewarePoolConnection) -> Self {
        BatchTarget::Connection(conn)
    }
}

#[cfg(feature = "postgres")]
impl<'a> From<&'a postgres::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a postgres::transaction::Tx<'a>) -> Self {
        BatchTarget::PostgresTx(tx)
    }
}

#[cfg(feature = "mssql")]
impl<'a> From<&'a mut mssql::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a mut mssql::transaction::Tx<'a>) -> Self {
        BatchTarget::MssqlTx(tx)
    }
}

#[cfg(feature = "turso")]
impl<'a> From<&'a turso::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a turso::transaction::Tx<'a>) -> Self {
        BatchTarget::TursoTx(tx)
    }
}

impl<'a> From<&'a mut MiddlewarePoolConnection> for QueryTarget<'a> {
    fn from(conn: &'a mut MiddlewarePoolConnection) -> Self {
        QueryTarget {
            translation_default: conn.translation_default(),
            statement_cache_mode: conn.statement_cache_mode_default(),
            kind: QueryTargetKind::Connection(conn),
        }
    }
}

#[cfg(feature = "postgres")]
impl<'a> From<&'a postgres::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a postgres::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            statement_cache_mode: StatementCacheMode::Cached,
            kind: QueryTargetKind::PostgresTx(tx),
        }
    }
}

#[cfg(feature = "mssql")]
impl<'a> From<&'a mut mssql::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a mut mssql::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            statement_cache_mode: StatementCacheMode::Cached,
            kind: QueryTargetKind::MssqlTx(tx),
        }
    }
}

#[cfg(feature = "turso")]
impl<'a> From<&'a turso::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a turso::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            statement_cache_mode: StatementCacheMode::Cached,
            kind: QueryTargetKind::TursoTx(tx),
        }
    }
}
