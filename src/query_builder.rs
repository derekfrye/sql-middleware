use std::borrow::Cow;

use crate::error::SqlMiddlewareDbError;
use crate::executor::{
    QueryTarget, QueryTargetKind, execute_dml_dispatch, execute_select_dispatch,
};
use crate::pool::MiddlewarePoolConnection;
use crate::results::ResultSet;
use crate::translation::{QueryOptions, TranslationMode, translate_placeholders};
use crate::types::RowValues;

/// Fluent builder for query execution with optional placeholder translation.
pub struct QueryBuilder<'conn, 'q> {
    target: QueryTarget<'conn>,
    sql: Cow<'q, str>,
    params: Cow<'q, [RowValues]>,
    options: QueryOptions,
}

impl<'conn, 'q> QueryBuilder<'conn, 'q> {
    pub(crate) fn new(conn: &'conn mut MiddlewarePoolConnection, sql: &'q str) -> Self {
        Self {
            target: conn.into(),
            sql: Cow::Borrowed(sql),
            params: Cow::Borrowed(&[]),
            options: QueryOptions::default(),
        }
    }

    pub(crate) fn new_target(target: QueryTarget<'conn>, sql: &'q str) -> Self {
        Self {
            target,
            sql: Cow::Borrowed(sql),
            params: Cow::Borrowed(&[]),
            options: QueryOptions::default(),
        }
    }

    /// Provide parameters for this statement.
    #[must_use]
    pub fn params(mut self, params: &'q [RowValues]) -> Self {
        self.params = Cow::Borrowed(params);
        self
    }

    /// Override translation using `QueryOptions`.
    #[must_use]
    pub fn options(mut self, options: QueryOptions) -> Self {
        self.options = options;
        self
    }

    /// Override translation mode directly.
    ///
    /// Warning: translation skips placeholders inside quoted strings, comments, and dollar-quoted
    /// blocks via a lightweight state machine; it may miss edge cases in complex SQL (e.g.,
    /// PL/pgSQL bodies). Prefer backend-specific SQL instead of relying on translation:
    /// ```rust
    /// # use sql_middleware::prelude::*;
    /// # async fn demo(conn: &mut MiddlewarePoolConnection) -> Result<(), SqlMiddlewareDbError> {
    /// let query = match conn {
    ///     MiddlewarePoolConnection::Postgres { .. } => r#"$function$
    /// BEGIN
    ///     RETURN ($1 ~ $q$[\t\r\n\v\\]$q$);
    /// END;
    /// $function$"#,
    ///     MiddlewarePoolConnection::Sqlite { .. } | MiddlewarePoolConnection::Turso { .. } => {
    ///         include_str!("../sql/functions/sqlite/03_sp_get_scores.sql")
    ///     }
    /// };
    /// # let _ = query;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn translation(mut self, translation: TranslationMode) -> Self {
        self.options.translation = translation;
        self
    }

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
            #[cfg(feature = "typed-sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqlite { conn },
                ..
            } => crate::typed::typed_sqlite_select(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqliteTx { conn },
                ..
            } => crate::typed::typed_sqlite_select(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgres { conn },
                ..
            } => crate::typed_postgres::select(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgresTx { conn },
                ..
            } => crate::typed_postgres::select(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTurso { conn },
                ..
            } => crate::typed_turso::select(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTursoTx { conn },
                ..
            } => crate::typed_turso::select(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::PostgresTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref()).await?;
                tx.query_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::SqliteTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref())?;
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
            #[cfg(feature = "typed-sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqlite { conn },
                ..
            } => crate::typed::typed_sqlite_dml(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-sqlite")]
            QueryTarget {
                kind: QueryTargetKind::TypedSqliteTx { conn },
                ..
            } => crate::typed::typed_sqlite_dml(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgres { conn },
                ..
            } => crate::typed_postgres::dml(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-postgres")]
            QueryTarget {
                kind: QueryTargetKind::TypedPostgresTx { conn },
                ..
            } => crate::typed_postgres::dml(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTurso { conn },
                ..
            } => crate::typed_turso::dml(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "typed-turso")]
            QueryTarget {
                kind: QueryTargetKind::TypedTursoTx { conn },
                ..
            } => crate::typed_turso::dml(conn, translated.as_ref(), self.params.as_ref())
                .await,
            #[cfg(feature = "postgres")]
            QueryTarget {
                kind: QueryTargetKind::PostgresTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref()).await?;
                tx.execute_prepared(&prepared, self.params.as_ref()).await
            }
            #[cfg(feature = "sqlite")]
            QueryTarget {
                kind: QueryTargetKind::SqliteTx(tx),
                ..
            } => {
                let prepared = tx.prepare(translated.as_ref())?;
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

fn translate_query_for_target<'a>(
    target: &QueryTarget<'_>,
    query: &'a str,
    params: &[RowValues],
    options: QueryOptions,
) -> Cow<'a, str> {
    if params.is_empty() {
        return Cow::Borrowed(query);
    }

    let Some(style) = target.translation_target() else {
        return Cow::Borrowed(query);
    };

    let enabled = options.translation.resolve(target.translation_default());
    translate_placeholders(query, style, enabled)
}
