use crate::error::SqlMiddlewareDbError;
use crate::executor::{
    QueryTarget, QueryTargetKind, execute_select_dispatch, execute_select_prepared_dispatch,
};
use crate::pool::MiddlewarePoolConnection;
use crate::translation::PrepareMode;
use crate::results::ResultSet;
use crate::types::RowValues;

#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;
#[cfg(feature = "sqlite")]
use crate::sqlite::config::SqliteManager;
#[cfg(feature = "turso")]
use crate::typed_turso::TursoManager;
#[cfg(any(feature = "postgres", feature = "sqlite", feature = "turso"))]
use bb8::PooledConnection;

use super::{QueryBuilder, translate_query_for_target};

impl QueryBuilder<'_, '_> {
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
        let use_prepare = matches!(self.options.prepare, PrepareMode::Prepared);

        match self.target {
            QueryTarget {
                kind: QueryTargetKind::Connection(conn),
                ..
            } => {
                select_on_connection(conn, translated.as_ref(), self.params.as_ref(), use_prepare)
                    .await
            }
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqlite { conn }
                    | QueryTargetKind::TypedSqliteTx { conn },
                ..
            } => select_typed_sqlite(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgres { conn }
                    | QueryTargetKind::TypedPostgresTx { conn },
                ..
            } => select_typed_postgres(conn, translated.as_ref(), self.params.as_ref(), use_prepare)
                .await,
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTurso { conn }
                    | QueryTargetKind::TypedTursoTx { conn },
                ..
            } => select_typed_turso(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::PostgresTx(tx),
                ..
            } => {
                if use_prepare {
                    let prepared = tx.prepare(translated.as_ref()).await?;
                    tx.query_prepared(&prepared, self.params.as_ref()).await
                } else {
                    tx.query(translated.as_ref(), self.params.as_ref()).await
                }
            }
            #[cfg(feature = "mssql")]
            QueryTarget {
                kind: QueryTargetKind::MssqlTx(tx),
                ..
            } => {
                if use_prepare {
                    let prepared = tx.prepare(translated.as_ref())?;
                    tx.query_prepared(&prepared, self.params.as_ref()).await
                } else {
                    tx.query(translated.as_ref(), self.params.as_ref()).await
                }
            }
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TursoTx(tx),
                ..
            } => {
                if use_prepare {
                    let mut prepared = tx.prepare(translated.as_ref()).await?;
                    tx.query_prepared(&mut prepared, self.params.as_ref()).await
                } else {
                    tx.execute_select(translated.as_ref(), self.params.as_ref()).await
                }
            }
        }
    }
}

async fn select_on_connection(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
    use_prepare: bool,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    if use_prepare {
        execute_select_prepared_dispatch(conn, query, params).await
    } else {
        execute_select_dispatch(conn, query, params).await
    }
}

#[cfg(feature = "sqlite")]
async fn select_typed_sqlite(
    conn: &mut PooledConnection<'static, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    crate::sqlite::connection::select(conn, query, params).await
}

#[cfg(feature = "postgres")]
async fn select_typed_postgres(
    conn: &mut PooledConnection<'static, PgManager>,
    query: &str,
    params: &[RowValues],
    use_prepare: bool,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    if use_prepare {
        crate::typed_postgres::select_prepared(conn, query, params).await
    } else {
        crate::typed_postgres::select(conn, query, params).await
    }
}

#[cfg(feature = "turso")]
async fn select_typed_turso(
    conn: &mut PooledConnection<'static, TursoManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    crate::typed_turso::select(conn, query, params).await
}
