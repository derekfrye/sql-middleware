use std::borrow::Cow;

use crate::error::SqlMiddlewareDbError;
use crate::executor::{execute_dml_dispatch, execute_select_dispatch, translate_query};
use crate::pool::MiddlewarePoolConnection;
use crate::results::ResultSet;
use crate::translation::{QueryOptions, TranslationMode};
use crate::types::RowValues;

/// Fluent builder for query execution with optional placeholder translation.
pub struct QueryBuilder<'conn, 'q> {
    conn: &'conn mut MiddlewarePoolConnection,
    sql: Cow<'q, str>,
    params: Cow<'q, [RowValues]>,
    options: QueryOptions,
}

impl<'conn, 'q> QueryBuilder<'conn, 'q> {
    pub(crate) fn new(conn: &'conn mut MiddlewarePoolConnection, sql: &'q str) -> Self {
        Self {
            conn,
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
    #[must_use]
    pub fn translation(mut self, translation: TranslationMode) -> Self {
        self.options.translation = translation;
        self
    }

    /// Execute a SELECT and return the result set.
    pub async fn select(self) -> Result<ResultSet, SqlMiddlewareDbError> {
        let translated = translate_query(
            self.conn,
            self.sql.as_ref(),
            self.params.as_ref(),
            self.options,
        );
        execute_select_dispatch(self.conn, translated.as_ref(), self.params.as_ref()).await
    }

    /// Execute a DML statement and return rows affected.
    pub async fn dml(self) -> Result<usize, SqlMiddlewareDbError> {
        let translated = translate_query(
            self.conn,
            self.sql.as_ref(),
            self.params.as_ref(),
            self.options,
        );
        execute_dml_dispatch(self.conn, translated.as_ref(), self.params.as_ref()).await
    }

    /// Execute a batch (ignores params by design).
    pub async fn batch(self) -> Result<(), SqlMiddlewareDbError> {
        self.conn.execute_batch(&self.sql).await
    }
}
