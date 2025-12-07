#![cfg(feature = "turso")]

use sql_middleware::prelude::*;

#[test]
fn turso_url_literal_vs_placeholder() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::turso_builder(":memory:".to_string())
            .build()
            .await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE tbl (val TEXT);").await?;
        conn.query("INSERT INTO tbl (val) VALUES (?1);")
            .params(&[RowValues::Text(
                "https://example.com/?1=param1Value&2=param2&token=$123abc".into(),
            )])
            .dml()
            .await?;

        let rs = conn
            .query("SELECT val FROM tbl WHERE val LIKE 'https://example.com/?1=' || ?1 || '%';")
            .params(&[RowValues::Text("param1Value".into())])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(
            rs.results[0].get("val").unwrap().as_text().unwrap(),
            "https://example.com/?1=param1Value&2=param2&token=$123abc"
        );
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

#[test]
fn turso_translates_postgres_style_when_forced_on() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        // Default translation off; per-call override forces $1 -> ?1.
        let cap = ConfigAndPool::turso_builder(":memory:".to_string())
            .translation(false)
            .build()
            .await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE t_force_on (id INTEGER, note TEXT);")
            .await?;

        let rs = conn
            .query("INSERT INTO t_force_on (id, note) VALUES ($1, 'literal $1 stays') RETURNING id, note;")
            .translation(TranslationMode::ForceOn)
            .params(&[RowValues::Int(21)])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(*rs.results[0].get("id").unwrap().as_int().unwrap(), 21);
        assert_eq!(
            rs.results[0].get("note").unwrap().as_text().unwrap(),
            "literal $1 stays"
        );
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

#[test]
fn turso_translation_force_off_with_pool_default_on() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        // Pool default on; per-call override should skip translation.
        let cap = ConfigAndPool::turso_builder(":memory:".to_string())
            .translation(true)
            .build()
            .await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE t_force_off (id INTEGER);")
            .await?;

        let rs = conn
            .query("INSERT INTO t_force_off (id) VALUES ($1) RETURNING id;")
            .translation(TranslationMode::ForceOff)
            .params(&[RowValues::Int(9)])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(*rs.results[0].get("id").unwrap().as_int().unwrap(), 9);
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

#[test]
fn turso_translation_skips_comments_and_literals() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        // Default off; override forces $n -> ?n for SQLite/Turso.
        let cap = ConfigAndPool::turso_builder(":memory:".to_string())
            .translation(false)
            .build()
            .await?;
        let mut conn = cap.get_connection().await?;

        // Comment should not be translated; $1 outside comment should be.
        let rs = conn
            .query("SELECT 1 /* ?1 in block comment */ + $1 -- $2 in line comment\n AS val;")
            .translation(TranslationMode::ForceOn)
            .params(&[RowValues::Int(3)])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(*rs.results[0].get("val").unwrap().as_int().unwrap(), 4);

        // Literal containing $1 stays untouched; $2 outside literal translates.
        let rs = conn
            .query("SELECT 'O''Reilly || ?1$1' || $2 AS val;")
            .translation(TranslationMode::ForceOn)
            .params(&[
                RowValues::Text("ignored".into()),
                RowValues::Text("Y".into()),
            ])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(
            rs.results[0].get("val").unwrap().as_text().unwrap(),
            "O'Reilly || ?1$1Y"
        );

        // Literal + param next to it should translate the param.
        let rs = conn
            .query("SELECT 'O''Reilly' || $1 AS val;")
            .translation(TranslationMode::ForceOn)
            .params(&[RowValues::Text("Y".into())])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(
            rs.results[0].get("val").unwrap().as_text().unwrap(),
            "O'ReillyY"
        );

        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
