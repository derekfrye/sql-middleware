use crate::pool::MiddlewarePoolConnection;
use crate::translation::PlaceholderStyle;

use super::targets::{QueryTarget, QueryTargetKind};

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
