use super::{QueryTarget, QueryTargetKind};
use crate::types::StatementCacheMode;

#[cfg(feature = "mssql")]
use crate::mssql::typed::MssqlManager;
#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;
#[cfg(feature = "sqlite")]
use crate::sqlite::config::SqliteManager;
#[cfg(feature = "turso")]
use crate::typed_turso::TursoManager;
#[cfg(any(
    feature = "postgres",
    feature = "turso",
    feature = "sqlite",
    feature = "mssql"
))]
use bb8::PooledConnection;

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
            statement_cache_mode: StatementCacheMode::Cached,
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
            statement_cache_mode: StatementCacheMode::Cached,
            kind,
        }
    }
}

#[cfg(feature = "mssql")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_mssql(
        conn: &'a mut PooledConnection<'static, MssqlManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedMssqlTx { conn }
        } else {
            QueryTargetKind::TypedMssql { conn }
        };
        QueryTarget {
            translation_default: false,
            statement_cache_mode: StatementCacheMode::Cached,
            kind,
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
            statement_cache_mode: StatementCacheMode::Cached,
            kind,
        }
    }
}
