use std::borrow::Cow;

/// Target placeholder style for translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderStyle {
    /// PostgreSQL-style placeholders like `$1`.
    Postgres,
    /// SQLite-style placeholders like `?1` (also used by LibSQL/Turso).
    Sqlite,
}

/// How to resolve translation for a call relative to the pool default.
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

/// Per-call options for query/execute paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryOptions {
    pub translation: TranslationMode,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            translation: TranslationMode::PoolDefault,
        }
    }
}

impl QueryOptions {
    #[must_use]
    pub fn with_translation(mut self, translation: TranslationMode) -> Self {
        self.translation = translation;
        self
    }
}

/// Translate placeholders between Postgres-style `$N` and SQLite-style `?N`.
///
/// Returns a borrowed `Cow` when no changes are needed.
#[must_use]
pub fn translate_placeholders<'a>(
    sql: &'a str,
    target: PlaceholderStyle,
    enabled: bool,
) -> Cow<'a, str> {
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
                b'-' if bytes.get(idx + 1) == Some(&b'-') => {
                    state = State::LineComment;
                    idx += 1;
                }
                b'/' if bytes.get(idx + 1) == Some(&b'*') => {
                    state = State::BlockComment(1);
                    idx += 1;
                }
                b'$' => {
                    if let Some((tag, advance)) = try_start_dollar_quote(bytes, idx) {
                        state = State::DollarQuoted(tag);
                        idx = advance;
                    } else if matches!(target, PlaceholderStyle::Sqlite) {
                        if let Some((digits_end, digits)) = scan_digits(bytes, idx + 1) {
                            out.get_or_insert_with(|| sql[..idx].to_string()).push('?');
                            out.as_mut().unwrap().push_str(digits);
                            idx = digits_end - 1;
                            replaced = true;
                        }
                    }
                }
                b'?' if matches!(target, PlaceholderStyle::Postgres) => {
                    if let Some((digits_end, digits)) = scan_digits(bytes, idx + 1) {
                        out.get_or_insert_with(|| sql[..idx].to_string()).push('$');
                        out.as_mut().unwrap().push_str(digits);
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
                if b == b'/' && bytes.get(idx + 1) == Some(&b'*') {
                    state = State::BlockComment(depth + 1);
                    idx += 1;
                } else if b == b'*' && bytes.get(idx + 1) == Some(&b'/') {
                    if depth == 1 {
                        state = State::Normal;
                    } else {
                        state = State::BlockComment(depth - 1);
                    }
                    idx += 1;
                }
            }
            State::DollarQuoted(ref tag) => {
                if b == b'$' && matches_tag(bytes, idx, tag) {
                    state = State::Normal;
                    idx += tag.len();
                }
            }
        }

        if let Some(ref mut buf) = out {
            if !replaced {
                buf.push(b as char);
            }
        }

        idx += 1;
    }

    match out {
        Some(buf) => Cow::Owned(buf),
        None => Cow::Borrowed(sql),
    }
}

#[derive(Clone)]
enum State {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment(u32),
    DollarQuoted(String),
}

fn scan_digits(bytes: &[u8], start: usize) -> Option<(usize, &str)> {
    let mut idx = start;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == start {
        None
    } else {
        std::str::from_utf8(&bytes[start..idx])
            .ok()
            .map(|digits| (idx, digits))
    }
}

fn try_start_dollar_quote(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut idx = start + 1;
    while idx < bytes.len() && bytes[idx] != b'$' {
        let b = bytes[idx];
        if !(b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        idx += 1;
    }

    if idx < bytes.len() && bytes[idx] == b'$' {
        let tag = String::from_utf8(bytes[start + 1..idx].to_vec()).ok()?;
        Some((tag, idx))
    } else {
        None
    }
}

fn matches_tag(bytes: &[u8], idx: usize, tag: &str) -> bool {
    let end = idx + 1 + tag.len();
    end < bytes.len()
        && bytes[idx + 1..=end].starts_with(tag.as_bytes())
        && bytes.get(end) == Some(&b'$')
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
        assert_eq!(res, "select '?1', $1 -- $2\n/* ?3 */ from t where a = ?1");
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
