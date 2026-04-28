#![cfg(feature = "postgres")]
#[path = "postgres_translation/typed.rs"]
mod typed;

use sql_middleware::prelude::*;
use std::env;

#[cfg(feature = "postgres")]
fn build_typed_pg_config(cfg: &PgConfig) -> tokio_postgres::Config {
    cfg.to_tokio_config()
}

#[test]
fn postgres_url_literal_vs_placeholder() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut cfg = PgConfig::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string()); // default user
        // Default to empty for trust auth, allow env override when a password is needed.
        cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());

        #[cfg(feature = "postgres")]
        let typed_pg_cfg = build_typed_pg_config(&cfg);
        let expected_url =
            "https://example.com/?1=param1Value&2=param2&token=$123abc".to_string();

        let cap = ConfigAndPool::new_postgres(PostgresOptions::new(cfg)).await?;
        let mut conn = cap.get_connection().await?;

        // Drop and recreate to ensure no leftover rows from previous runs against the shared DB
        conn.execute_batch("DROP TABLE IF EXISTS tbl; CREATE TABLE tbl (val TEXT);")
            .await?;
        conn.query("INSERT INTO tbl (val) VALUES ($1);")
            .params(&[RowValues::Text(
                "https://example.com/?1=param1Value&2=param2&token=$123abc".into(),
            )])
            .dml()
            .await?;

        let rs = conn
            .query(
                "SELECT val FROM tbl WHERE val LIKE $tag$https://example.com/?1=$tag$ || $1 || '%';",
            )
            .params(&[RowValues::Text("param1Value".into())])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(
            rs.results[0].get("val").unwrap().as_text().unwrap(),
            expected_url.as_str()
        );

        #[cfg(feature = "postgres")]
        typed::url_literal_vs_placeholder(&typed_pg_cfg, expected_url.as_str()).await?;

        conn.execute_batch("DROP TABLE IF EXISTS tbl;").await?;
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

#[test]
fn postgres_translation_force_on_override_default_off() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut cfg = PgConfig::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string());
        cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());

        #[cfg(feature = "postgres")]
        let typed_pg_cfg = build_typed_pg_config(&cfg);

        // Pool default is off; per-call override forces translation of ?1 -> $1.
        let cap = ConfigAndPool::new_postgres(PostgresOptions::new(cfg)).await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch(
            "DROP TABLE IF EXISTS tbl_translate_force_on;
             CREATE TABLE tbl_translate_force_on (id BIGINT);",
        )
        .await?;

        let rs = conn
            .query("INSERT INTO tbl_translate_force_on (id) VALUES (?1) RETURNING id;")
            .translation(TranslationMode::ForceOn)
            .params(&[RowValues::Int(7)])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(*rs.results[0].get("id").unwrap().as_int().unwrap(), 7);

        #[cfg(feature = "postgres")]
        typed::translation_force_on(&typed_pg_cfg).await?;

        conn.execute_batch("DROP TABLE IF EXISTS tbl_translate_force_on;")
            .await?;
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

#[test]
fn postgres_translation_force_off_with_pool_default_on() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut cfg = PgConfig::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string());
        cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());

        #[cfg(feature = "postgres")]
        let typed_pg_cfg = build_typed_pg_config(&cfg);

        // Pool default is on; per-call override should skip translation, leaving ?1 invalid.
        let cap =
            ConfigAndPool::new_postgres(PostgresOptions::new(cfg).with_translation(true)).await?;
        let mut conn = cap.get_connection().await?;

        let res = conn
            .query("SELECT ?1")
            .translation(TranslationMode::ForceOff)
            .params(&[RowValues::Int(1)])
            .select()
            .await;

        assert!(res.is_err(), "expected SQL to fail without translation");

        #[cfg(feature = "postgres")]
        typed::translation_force_off(&typed_pg_cfg).await?;
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

#[test]
fn postgres_translation_skips_comments_and_literals() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut cfg = PgConfig::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string());
        cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());

        #[cfg(feature = "postgres")]
        let typed_pg_cfg = build_typed_pg_config(&cfg);

        let cap = ConfigAndPool::new_postgres(PostgresOptions::new(cfg)).await?;
        let mut conn = cap.get_connection().await?;

        // Comment should not be translated; ?1 outside comment should be.
        let rs = conn
            .query("SELECT 1 -- $1 in comment\n                 + ?1 AS val;")
            .translation(TranslationMode::ForceOn)
            .params(&[RowValues::Int(1)])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(*rs.results[0].get("val").unwrap().as_int().unwrap(), 2);

        // Literal containing ?1 stays untouched; ?1 outside literal translates.
        let rs = conn
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

        // Literal + param next to it should translate the param.
        let rs = conn
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

        #[cfg(feature = "postgres")]
        typed::translation_skip_comments_and_literals(&typed_pg_cfg).await?;

        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
