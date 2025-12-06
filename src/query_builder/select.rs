use crate::error::SqlMiddlewareDbError;
use crate::executor::{execute_select_dispatch, QueryTarget, QueryTargetKind};
use crate::results::ResultSet;

use super::{translate_query_for_target, QueryBuilder};

impl<'conn, 'q> QueryBuilder<'conn, 'q> {
    /// Execute a SELECT and return the result set.
    ///
    /// # Errors
    /// Returns an error if placeholder translation fails or the backend query execution fails.
    pub async fn select(self) -> Result<ResultSet, SqlMiddlewareDbError> {
        let translated = translate_query_for_target(
            &self.target,
            self.sql.as_ref(),
            self.params.as_ref(),
            self.options,
        );

        match self.target {
            QueryTarget {
                kind: QueryTargetKind::Connection(conn),
                ..
            } => execute_select_dispatch(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqlite { conn },
                ..
            } => {
                crate::sqlite::connection::select(conn, translated.as_ref(), self.params.as_ref())
                    .await
            }
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqliteTx { conn },
                ..
            } => {
                crate::sqlite::connection::select(conn, translated.as_ref(), self.params.as_ref())
                    .await
            }
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgres { conn },
                ..
            } => {
                crate::typed_postgres::select(conn, translated.as_ref(), self.params.as_ref()).await
            }
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgresTx { conn },
                ..
            } => {
                crate::typed_postgres::select(conn, translated.as_ref(), self.params.as_ref()).await
            }
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTurso { conn },
                ..
            } => crate::typed_turso::select(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTursoTx { conn },
                ..
            } => crate::typed_turso::select(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::PostgresTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref()).await?;
                tx.query_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "mssql")]
            QueryTarget {
                kind: QueryTargetKind::MssqlTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref())?;
                tx.query_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "libsql")]
            QueryTarget {
                kind: QueryTargetKind::LibsqlTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref())?;
                tx.query_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TursoTx(tx),
                ..
            } => {
                let mut prepared = tx.prepare(translated.as_ref()).await?;
                tx.query_prepared(&mut prepared, self.params.as_ref()).await
            }
        }
    }
}
