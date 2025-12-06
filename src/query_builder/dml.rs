use crate::error::SqlMiddlewareDbError;
use crate::executor::{execute_dml_dispatch, QueryTarget, QueryTargetKind};

use super::{translate_query_for_target, QueryBuilder};

impl<'conn, 'q> QueryBuilder<'conn, 'q> {
    /// Execute a DML statement and return rows affected.
    ///
    /// # Errors
    /// Returns an error if placeholder translation fails or the backend DML execution fails.
    pub async fn dml(self) -> Result<usize, SqlMiddlewareDbError> {
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
            } => execute_dml_dispatch(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqlite { conn },
                ..
            } => {
                crate::sqlite::connection::dml(conn, translated.as_ref(), self.params.as_ref())
                    .await
            }
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqliteTx { conn },
                ..
            } => {
                crate::sqlite::connection::dml(conn, translated.as_ref(), self.params.as_ref())
                    .await
            }
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgres { conn },
                ..
            } => crate::typed_postgres::dml(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgresTx { conn },
                ..
            } => crate::typed_postgres::dml(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTurso { conn },
                ..
            } => crate::typed_turso::dml(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTursoTx { conn },
                ..
            } => crate::typed_turso::dml(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::PostgresTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref()).await?;
                tx.execute_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "mssql")]
            QueryTarget {
                kind: QueryTargetKind::MssqlTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref())?;
                tx.execute_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "libsql")]
            QueryTarget {
                kind: QueryTargetKind::LibsqlTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref())?;
                tx.execute_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TursoTx(tx),
                ..
            } => {
                let mut prepared = tx.prepare(translated.as_ref()).await?;
                tx.execute_prepared(&mut prepared, self.params.as_ref())
                    .await
            }
        }
    }
}
