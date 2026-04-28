use sql_middleware::prelude::*;
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};

pub(super) async fn url_literal_vs_placeholder(
    cfg: &tokio_postgres::Config,
    expected_url: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let pool = PgManager::new(cfg.clone()).build_pool().await?;
    let mut typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;
    let typed_rs = typed_conn
        .select(
            "SELECT val FROM tbl WHERE val LIKE $tag$https://example.com/?1=$tag$ || $1 || '%';",
            &[RowValues::Text("param1Value".into())],
        )
        .await?;
    assert_eq!(typed_rs.results.len(), 1);
    assert_eq!(
        typed_rs.results[0].get("val").unwrap().as_text().unwrap(),
        expected_url
    );
    Ok(())
}

pub(super) async fn translation_force_on(
    cfg: &tokio_postgres::Config,
) -> Result<(), SqlMiddlewareDbError> {
    let pool = PgManager::new(cfg.clone()).build_pool().await?;
    let mut typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;
    typed_conn
        .execute_batch(
            "DROP TABLE IF EXISTS tbl_translate_force_on;
             CREATE TABLE tbl_translate_force_on (id BIGINT);",
        )
        .await?;
    let typed_rs = typed_conn
        .query("INSERT INTO tbl_translate_force_on (id) VALUES (?1) RETURNING id;")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Int(8)])
        .select()
        .await?;
    assert_eq!(typed_rs.results.len(), 1);
    assert_eq!(*typed_rs.results[0].get("id").unwrap().as_int().unwrap(), 8);
    typed_conn
        .execute_batch("DROP TABLE IF EXISTS tbl_translate_force_on;")
        .await?;
    Ok(())
}

pub(super) async fn translation_force_off(
    cfg: &tokio_postgres::Config,
) -> Result<(), SqlMiddlewareDbError> {
    let pool = PgManager::new(cfg.clone()).build_pool().await?;
    let mut typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;
    let res = typed_conn
        .query("SELECT ?1")
        .translation(TranslationMode::ForceOff)
        .params(&[RowValues::Int(1)])
        .select()
        .await;
    assert!(
        res.is_err(),
        "expected typed-postgres SQL to fail without translation"
    );
    Ok(())
}

pub(super) async fn translation_skip_comments_and_literals(
    cfg: &tokio_postgres::Config,
) -> Result<(), SqlMiddlewareDbError> {
    let pool = PgManager::new(cfg.clone()).build_pool().await?;
    let mut typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;
    assert_comment_translation(&mut typed_conn).await?;
    assert_literal_translation(&mut typed_conn).await?;
    Ok(())
}

async fn assert_comment_translation(
    typed_conn: &mut PgConnection<PgIdle>,
) -> Result<(), SqlMiddlewareDbError> {
    let rs = typed_conn
        .query("SELECT 1 -- $1 in comment\n                 + ?1 AS val;")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Int(1)])
        .select()
        .await?;
    assert_eq!(rs.results.len(), 1);
    assert_eq!(*rs.results[0].get("val").unwrap().as_int().unwrap(), 2);
    Ok(())
}

async fn assert_literal_translation(
    typed_conn: &mut PgConnection<PgIdle>,
) -> Result<(), SqlMiddlewareDbError> {
    let rs = typed_conn
        .query("SELECT 'O''Reilly || ?1' || ?1 AS val;")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Text("X".into())])
        .select()
        .await?;
    assert_eq!(rs.results.len(), 1);
    assert_eq!(
        rs.results[0].get("val").unwrap().as_text().unwrap(),
        "O'Reilly || ?1X"
    );
    let rs = typed_conn
        .query("SELECT 'O''Reilly' || ?1 AS val;")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Text("X".into())])
        .select()
        .await?;
    assert_eq!(rs.results.len(), 1);
    assert_eq!(
        rs.results[0].get("val").unwrap().as_text().unwrap(),
        "O'ReillyX"
    );
    Ok(())
}
