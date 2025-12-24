use crate::pool::MiddlewarePoolConnection;
use crate::translation::PlaceholderStyle;

#[cfg(feature = "mssql")]
use crate::mssql;
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
#[cfg(any(feature = "postgres", feature = "turso", feature = "sqlite"))]
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
    translation_default: bool,
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
            kind: QueryTargetKind::Connection(conn),
        }
    }
}

#[cfg(feature = "sqlite")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_sqlite(
        conn: &'a mut PooledConnection<'static, SqliteManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedSqliteTx { conn }
        } else {
            QueryTargetKind::TypedSqlite { conn }
        };
        QueryTarget {
            translation_default: false,
            kind,
        }
    }
}

#[cfg(feature = "postgres")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_postgres(
        conn: &'a mut PooledConnection<'static, PgManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedPostgresTx { conn }
        } else {
            QueryTargetKind::TypedPostgres { conn }
        };
        QueryTarget {
            translation_default: false,
            kind,
        }
    }
}

#[cfg(feature = "postgres")]
impl<'a> From<&'a postgres::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a postgres::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::PostgresTx(tx),
        }
    }
}

#[cfg(feature = "mssql")]
impl<'a> From<&'a mut mssql::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a mut mssql::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::MssqlTx(tx),
        }
    }
}

#[cfg(feature = "turso")]
impl<'a> From<&'a turso::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a turso::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::TursoTx(tx),
        }
    }
}

#[cfg(feature = "turso")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_turso(
        conn: &'a mut PooledConnection<'static, TursoManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedTursoTx { conn }
        } else {
            QueryTargetKind::TypedTurso { conn }
        };
        QueryTarget {
            translation_default: true,
            kind,
        }
    }
}

impl QueryTarget<'_> {
    #[must_use]
    pub(crate) fn translation_default(&self) -> bool {
        self.translation_default
    }

    #[must_use]
    pub(crate) fn translation_target(&self) -> Option<PlaceholderStyle> {
        match &self.kind {
            QueryTargetKind::Connection(conn) => translation_target(conn),
            #[cfg(feature = "postgres")]
            QueryTargetKind::PostgresTx(_) => Some(PlaceholderStyle::Postgres),
            #[cfg(feature = "sqlite")]
            QueryTargetKind::TypedSqlite { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "sqlite")]
            QueryTargetKind::TypedSqliteTx { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "postgres")]
            QueryTargetKind::TypedPostgres { .. } => Some(PlaceholderStyle::Postgres),
            #[cfg(feature = "postgres")]
            QueryTargetKind::TypedPostgresTx { .. } => Some(PlaceholderStyle::Postgres),
            #[cfg(feature = "turso")]
            QueryTargetKind::TursoTx(_) => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "turso")]
            QueryTargetKind::TypedTurso { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "turso")]
            QueryTargetKind::TypedTursoTx { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "mssql")]
            QueryTargetKind::MssqlTx(_) => None,
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}

pub(crate) fn translation_target(conn: &MiddlewarePoolConnection) -> Option<PlaceholderStyle> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres { .. } => Some(PlaceholderStyle::Postgres),
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql { .. } => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}
