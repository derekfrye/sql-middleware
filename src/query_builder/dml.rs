use crate::error::SqlMiddlewareDbError;
use crate::executor::{
    QueryTarget, QueryTargetKind, execute_dml_dispatch, execute_dml_prepared_dispatch,
};
use crate::pool::MiddlewarePoolConnection;
use crate::translation::PrepareMode;
use crate::types::{RowValues, StatementCacheMode};

#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;
#[cfg(feature = "sqlite")]
use crate::sqlite::config::SqliteManager;
#[cfg(feature = "mssql")]
use crate::typed_mssql::MssqlManager;
#[cfg(feature = "turso")]
use crate::typed_turso::TursoManager;
#[cfg(any(
    feature = "postgres",
    feature = "sqlite",
    feature = "turso",
    feature = "mssql"
))]
use bb8::PooledConnection;

use super::{QueryBuilder, translate_query_for_target};

impl QueryBuilder<'_, '_> {
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
        let use_prepare = matches!(self.options.prepare, PrepareMode::Prepared);
        let statement_cache_mode = self
            .options
            .statement_cache
            .unwrap_or(self.target.statement_cache_mode);

        match self.target {
            QueryTarget {
                kind: QueryTargetKind::Connection(conn),
                ..
            } => {
                dml_on_connection(
                    conn,
                    translated.as_ref(),
                    self.params.as_ref(),
                    use_prepare,
                    statement_cache_mode,
                )
                .await
            }
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind:
                    QueryTargetKind::TypedSqlite { conn } | QueryTargetKind::TypedSqliteTx { conn },
                ..
            } => {
                dml_typed_sqlite(
                    conn,
                    translated.as_ref(),
                    self.params.as_ref(),
                    statement_cache_mode,
                )
                .await
            }
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind:
                    QueryTargetKind::TypedPostgres { conn } | QueryTargetKind::TypedPostgresTx { conn },
                ..
            } => {
                dml_typed_postgres(conn, translated.as_ref(), self.params.as_ref(), use_prepare)
                    .await
            }
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTurso { conn } | QueryTargetKind::TypedTursoTx { conn },
                ..
            } => {
                dml_typed_turso(
                    conn,
                    translated.as_ref(),
                    self.params.as_ref(),
                    statement_cache_mode,
                )
                .await
            }
            #[cfg(feature = "mssql")]
            QueryTarget {
                kind: QueryTargetKind::TypedMssql { conn } | QueryTargetKind::TypedMssqlTx { conn },
                ..
            } => dml_typed_mssql(conn, translated.as_ref(), self.params.as_ref()).await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::PostgresTx(tx),
                ..
            } => {
                if use_prepare {
                    let prepared = tx.prepare(translated.as_ref()).await?;
                    tx.execute(&prepared)
                        .params(self.params.as_ref())
                        .run()
                        .await
                } else {
                    tx.execute_dml(translated.as_ref(), self.params.as_ref())
                        .await
                }
            }
            #[cfg(feature = "mssql")]
            QueryTarget {
                kind: QueryTargetKind::MssqlTx(tx),
                ..
            } => {
                if use_prepare {
                    let prepared = tx.prepare(translated.as_ref())?;
                    tx.execute(&prepared)
                        .params(self.params.as_ref())
                        .run()
                        .await
                } else {
                    tx.execute_dml(translated.as_ref(), self.params.as_ref())
                        .await
                }
            }
            #[cfg(feature = "turso")]
            QueryTarget {
                kind: QueryTargetKind::TursoTx(tx),
                ..
            } => {
                if use_prepare {
                    let mut prepared = tx.prepare(translated.as_ref()).await?;
                    tx.execute(&mut prepared)
                        .params(self.params.as_ref())
                        .run()
                        .await
                } else {
                    tx.execute_dml(translated.as_ref(), self.params.as_ref())
                        .await
                }
            }
        }
    }
}

async fn dml_on_connection(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
    use_prepare: bool,
    statement_cache_mode: StatementCacheMode,
) -> Result<usize, SqlMiddlewareDbError> {
    if use_prepare {
        execute_dml_prepared_dispatch(conn, query, params, statement_cache_mode).await
    } else {
        execute_dml_dispatch(conn, query, params, statement_cache_mode).await
    }
}

#[cfg(feature = "sqlite")]
async fn dml_typed_sqlite(
    conn: &mut PooledConnection<'static, SqliteManager>,
    query: &str,
    params: &[RowValues],
    statement_cache_mode: StatementCacheMode,
) -> Result<usize, SqlMiddlewareDbError> {
    crate::sqlite::connection::dml_with_cache_mode(conn, query, params, statement_cache_mode).await
}

#[cfg(feature = "postgres")]
async fn dml_typed_postgres(
    conn: &mut PooledConnection<'static, PgManager>,
    query: &str,
    params: &[RowValues],
    use_prepare: bool,
) -> Result<usize, SqlMiddlewareDbError> {
    if use_prepare {
        crate::typed_postgres::dml_prepared(conn, query, params).await
    } else {
        crate::typed_postgres::dml(conn, query, params).await
    }
}

#[cfg(feature = "turso")]
async fn dml_typed_turso(
    conn: &mut PooledConnection<'static, TursoManager>,
    query: &str,
    params: &[RowValues],
    _statement_cache_mode: StatementCacheMode,
) -> Result<usize, SqlMiddlewareDbError> {
    crate::typed_turso::dml(conn, query, params).await
}

#[cfg(feature = "mssql")]
async fn dml_typed_mssql(
    conn: &mut PooledConnection<'static, MssqlManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    crate::typed_mssql::dml(conn, query, params).await
}
