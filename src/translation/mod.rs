use std::borrow::Cow;

mod parsers;
mod scanner;
#[cfg(test)]
mod tests;
mod translate;

/// Target placeholder style for translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderStyle {
    /// PostgreSQL-style placeholders like `$1`.
    Postgres,
    /// SQLite-style placeholders like `?1` (also used by Turso).
    Sqlite,
}

/// How to resolve translation for a call relative to the pool default.
///
/// # Examples
/// ```rust
/// use sql_middleware::prelude::*;
///
/// let options = QueryOptions::default()
///     .with_translation(TranslationMode::ForceOn);
/// # let _ = options;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationMode {
    /// Follow the pool's default setting.
    PoolDefault,
    /// Force translation on, regardless of pool default.
    ForceOn,
    /// Force translation off, regardless of pool default.
    ForceOff,
}

impl TranslationMode {
    #[must_use]
    pub fn resolve(self, pool_default: bool) -> bool {
        match self {
            TranslationMode::PoolDefault => pool_default,
            TranslationMode::ForceOn => true,
            TranslationMode::ForceOff => false,
        }
    }
}

/// How to resolve prepared execution for a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrepareMode {
    /// Execute without preparing.
    #[default]
    Direct,
    /// Prepare the statement before execution.
    Prepared,
}

/// Per-call options for query/execute paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryOptions {
    pub translation: TranslationMode,
    pub prepare: PrepareMode,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            translation: TranslationMode::PoolDefault,
            prepare: PrepareMode::default(),
        }
    }
}

impl QueryOptions {
    #[must_use]
    pub fn with_translation(mut self, translation: TranslationMode) -> Self {
        self.translation = translation;
        self
    }

    #[must_use]
    pub fn with_prepare(mut self, prepare: PrepareMode) -> Self {
        self.prepare = prepare;
        self
    }
}

/// Translate placeholders between Postgres-style `$N` and SQLite-style `?N`.
///
/// Warning: translation skips quoted strings, comments, and dollar-quoted blocks via a lightweight
/// state machine; it may still miss edge cases in complex SQL. For dialect-specific SQL (e.g.,
/// PL/pgSQL bodies), prefer backend-specific SQL instead of relying on translation:
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
///         "SELECT regexp_replace(body, ?1, ?2) FROM rules"
///     }
///     MiddlewarePoolConnection::Mssql { .. } => {
///         "SELECT REPLACE(body, @P1, @P2) FROM rules"
///     }
/// };
/// # let _ = query;
/// # Ok(())
/// # }
/// ```
/// Returns a borrowed `Cow` when no changes are needed.
#[must_use]
pub fn translate_placeholders(sql: &str, target: PlaceholderStyle, enabled: bool) -> Cow<'_, str> {
    translate::translate_sql(sql, target, enabled)
}
