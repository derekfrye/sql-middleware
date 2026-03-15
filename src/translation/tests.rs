use std::borrow::Cow;

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
