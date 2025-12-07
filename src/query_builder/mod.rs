use std::borrow::Cow;

use crate::executor::QueryTarget;
use crate::pool::MiddlewarePoolConnection;
use crate::translation::{QueryOptions, TranslationMode, translate_placeholders};
use crate::types::RowValues;

mod dml;
mod select;

/// Fluent builder for query execution with optional placeholder translation.
pub struct QueryBuilder<'conn, 'q> {
    pub(crate) target: QueryTarget<'conn>,
    pub(crate) sql: Cow<'q, str>,
    pub(crate) params: Cow<'q, [RowValues]>,
    pub(crate) options: QueryOptions,
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
}

pub(super) fn translate_query_for_target<'a>(
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
