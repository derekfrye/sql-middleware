use std::borrow::Cow;

mod parsers;
mod scanner;

use parsers::{
    is_block_comment_end, is_block_comment_start, is_line_comment_start, matches_tag,
    try_start_dollar_quote,
};
use scanner::{State, scan_digits};

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
///         include_str!("../sql/functions/sqlite/03_sp_get_scores.sql")
///     }
/// };
/// # let _ = query;
/// # Ok(())
/// # }
/// ```
/// Returns a borrowed `Cow` when no changes are needed.
#[must_use]
pub fn translate_placeholders(sql: &str, target: PlaceholderStyle, enabled: bool) -> Cow<'_, str> {
    if !enabled {
        return Cow::Borrowed(sql);
    }

    let mut out: Option<String> = None;
    let mut state = State::Normal;
    let mut idx = 0;
    let bytes = sql.as_bytes();

    while idx < bytes.len() {
        let b = bytes[idx];
        let mut replaced = false;
        match state {
            State::Normal => match b {
                b'\'' => state = State::SingleQuoted,
                b'"' => state = State::DoubleQuoted,
                _ if is_line_comment_start(bytes, idx) => state = State::LineComment,
                _ if is_block_comment_start(bytes, idx) => state = State::BlockComment(1),
                b'$' => {
                    if let Some((tag, advance)) = try_start_dollar_quote(bytes, idx) {
                        state = State::DollarQuoted(tag);
                        idx = advance;
                    } else if matches!(target, PlaceholderStyle::Sqlite)
                        && let Some((digits_end, digits)) = scan_digits(bytes, idx + 1)
                    {
                        let buf = out.get_or_insert_with(|| sql[..idx].to_string());
                        buf.push('?');
                        buf.push_str(digits);
                        idx = digits_end - 1;
                        replaced = true;
                    }
                }
                b'?' if matches!(target, PlaceholderStyle::Postgres) => {
                    if let Some((digits_end, digits)) = scan_digits(bytes, idx + 1) {
                        let buf = out.get_or_insert_with(|| sql[..idx].to_string());
                        buf.push('$');
                        buf.push_str(digits);
                        idx = digits_end - 1;
                        replaced = true;
                    }
                }
                _ => {}
            },
            State::SingleQuoted => {
                if b == b'\'' {
                    if bytes.get(idx + 1) == Some(&b'\'') {
                        idx += 1; // skip escaped quote
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::DoubleQuoted => {
                if b == b'"' {
                    if bytes.get(idx + 1) == Some(&b'"') {
                        idx += 1; // skip escaped quote
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::LineComment => {
                if b == b'\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment(depth) => {
                if is_block_comment_start(bytes, idx) {
                    state = State::BlockComment(depth + 1);
                } else if is_block_comment_end(bytes, idx) {
                    if depth == 1 {
                        state = State::Normal;
                    } else {
                        state = State::BlockComment(depth - 1);
                    }
                }
            }
            State::DollarQuoted(ref tag) => {
                if b == b'$' && matches_tag(bytes, idx, tag) {
                    let tag_len = tag.len();
                    state = State::Normal;
                    idx += tag_len;
                }
            }
        }

        if let Some(ref mut buf) = out
            && !replaced
        {
            buf.push(b as char);
        }

        idx += 1;
    }

    match out {
        Some(buf) => Cow::Owned(buf),
        None => Cow::Borrowed(sql),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_sqlite_to_postgres() {
        let sql = "select * from t where a = ?1 and b = ?2";
        let res = translate_placeholders(sql, PlaceholderStyle::Postgres, true);
        assert_eq!(res, "select * from t where a = $1 and b = $2");
    }

    #[test]
    fn translates_postgres_to_sqlite() {
        let sql = "insert into t values($1, $2)";
        let res = translate_placeholders(sql, PlaceholderStyle::Sqlite, true);
        assert_eq!(res, "insert into t values(?1, ?2)");
    }

    #[test]
    fn skips_inside_literals_and_comments() {
        let sql = "select '?1', $1 -- $2\n/* ?3 */ from t where a = $1";
        let res = translate_placeholders(sql, PlaceholderStyle::Sqlite, true);
        assert_eq!(res, "select '?1', ?1 -- $2\n/* ?3 */ from t where a = ?1");
    }

    #[test]
    fn skips_dollar_quoted_blocks() {
        let sql = "$foo$ select $1 from t $foo$ where a = $1";
        let res = translate_placeholders(sql, PlaceholderStyle::Sqlite, true);
        assert_eq!(res, "$foo$ select $1 from t $foo$ where a = ?1");
    }

    #[test]
    fn respects_disabled_flag() {
        let sql = "select * from t where a = ?1";
        let res = translate_placeholders(sql, PlaceholderStyle::Postgres, false);
        assert!(matches!(res, Cow::Borrowed(_)));
        assert_eq!(res, sql);
    }

    #[test]
    fn translation_mode_resolution() {
        assert!(TranslationMode::ForceOn.resolve(false));
        assert!(!TranslationMode::ForceOff.resolve(true));
        assert!(TranslationMode::PoolDefault.resolve(true));
        assert!(!TranslationMode::PoolDefault.resolve(false));
    }
}
