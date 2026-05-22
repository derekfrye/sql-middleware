#![cfg(any(feature = "sqlite", feature = "turso"))]
use chrono::NaiveDateTime;
use serde_json::json;
use sql_middleware::middleware::{
    ConfigAndPool, RowValues, SqlMiddlewareDbError, StatementCacheMode,
};
use tokio::runtime::Runtime;

enum TestCase {
    Sqlite(String),
    #[cfg(feature = "turso")]
    Turso(String),
}

fn unique_path(prefix: &str) -> String {
    let pid = std::process::id();
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{pid}_{ns}.db")
}

struct FileCleanup(Vec<String>);

impl Drop for FileCleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_file(format!("{p}-wal"));
            let _ = std::fs::remove_file(format!("{p}-shm"));
        }
    }
}

#[test]
fn sqlite_and_turso_core_logic() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    let test_cases = test_cases();

    for case in test_cases {
        // Clean up files for file-backed cases
        let _cleanup_guard = match &case {
            TestCase::Sqlite(path) if path != "file::memory:?cache=shared" => {
                let _ = std::fs::remove_file(path);
                Some(FileCleanup(vec![path.clone()]))
            }
            #[cfg(feature = "turso")]
            TestCase::Turso(path) if path != ":memory:" => {
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(format!("{path}-wal"));
                let _ = std::fs::remove_file(format!("{path}-shm"));
                Some(FileCleanup(vec![path.clone()]))
            }
            _ => None,
        };

        rt.block_on(async {
            // Build config/pool
            let cap = match case {
                TestCase::Sqlite(path) => ConfigAndPool::sqlite_builder(path).build().await?,
                #[cfg(feature = "turso")]
                TestCase::Turso(path) => ConfigAndPool::turso_builder(path).build().await?,
            };

            let mut conn = cap.get_connection().await?;

            // DDL: ensure table exists
            let ddl = r"
                CREATE TABLE IF NOT EXISTS test (
                    recid INTEGER PRIMARY KEY AUTOINCREMENT,
                    a int,
                    b text,
                    c datetime not null default current_timestamp,
                    d real,
                    e boolean,
                    f blob,
                    g json
                );
            ";
            conn.execute_batch(ddl).await?;

            // Seed with the shared SQLite setup script (SQLite-compatible)
            let setup = include_str!("../tests/sqlite/test2_setup.sql");
            conn.execute_batch(setup).await?;

            // Query a subset by recid
            let query = "SELECT * from test where recid in (?1, ?2, ?3);";
            let params = [RowValues::Int(1), RowValues::Int(2), RowValues::Int(3)];
            let res = conn.query(query).params(&params).select().await?;

            assert_eq!(res.results.len(), 3);

            // Validate row 1
            assert_eq!(*res.results[0].get("recid").unwrap().as_int().unwrap(), 1);
            assert_eq!(*res.results[0].get("a").unwrap().as_int().unwrap(), 1);
            assert_eq!(res.results[0].get("b").unwrap().as_text().unwrap(), "Alpha");
            assert_eq!(
                res.results[0]
                    .get("c")
                    .and_then(RowValues::as_timestamp)
                    .unwrap(),
                NaiveDateTime::parse_from_str("2024-01-01 08:00:01", "%Y-%m-%d %H:%M:%S").unwrap()
            );
            assert!(
                (res.results[0].get("d").unwrap().as_float().unwrap() - 10.5).abs() < f64::EPSILON
            );
            assert!(*res.results[0].get("e").unwrap().as_bool().unwrap());
            assert_eq!(
                res.results[0].get("f").unwrap().as_blob().unwrap(),
                b"Blob12"
            );
            // JSON is stored as text in this test data; confirm content
            assert_eq!(
                json!(res.results[0].get("g").unwrap().as_text().unwrap()),
                json!(r#"{"name": "Alice", "age": 30}"#)
            );

            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
    }

    Ok(())
}

#[test]
fn sqlite_and_turso_statement_cache_modes() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;

    rt.block_on(async {
        let sqlite_path = unique_path("cache_mode_sqlite");
        let _sqlite_cleanup = FileCleanup(vec![sqlite_path.clone()]);
        let cap = ConfigAndPool::sqlite_builder(sqlite_path)
            .statement_cache(StatementCacheMode::Uncached)
            .build()
            .await?;
        let mut conn = cap.get_connection().await?;
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)")
            .await?;
        conn.query("INSERT INTO t (id, name) VALUES (?1, ?2)")
            .params(&[RowValues::Int(1), RowValues::Text("sqlite".into())])
            .statement_cache(StatementCacheMode::Cached)
            .dml()
            .await?;
        let row = conn
            .query("SELECT name FROM t WHERE id = ?1")
            .params(&[RowValues::Int(1)])
            .prepare()
            .statement_cache(StatementCacheMode::Uncached)
            .select()
            .await?
            .into_one()?;
        assert_eq!(row.get("name").unwrap().as_text().unwrap(), "sqlite");

        #[cfg(feature = "turso")]
        {
            let cap = ConfigAndPool::turso_builder(":memory:".to_string())
                .statement_cache(StatementCacheMode::Uncached)
                .build()
                .await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)")
                .await?;
            conn.query("INSERT INTO t (id, name) VALUES (?1, ?2)")
                .params(&[RowValues::Int(1), RowValues::Text("turso".into())])
                .dml()
                .await?;
            let row = conn
                .query("SELECT name FROM t WHERE id = ?1")
                .params(&[RowValues::Int(1)])
                .prepare()
                .statement_cache(StatementCacheMode::Cached)
                .select()
                .await?
                .into_one()?;
            assert_eq!(row.get("name").unwrap().as_text().unwrap(), "turso");
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}

#[test]
fn sqlite_statement_cache_capacity_builder() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::sqlite_builder(
            "file:sqlite-cache-capacity?mode=memory&cache=shared".to_string(),
        )
        .statement_cache_capacity(0)
        .build()
        .await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, name TEXT);")
            .await?;
        conn.query("INSERT INTO t (id, name) VALUES (?1, ?2)")
            .params(&[RowValues::Int(1), RowValues::Text("alice".into())])
            .dml()
            .await?;
        let rs = conn
            .query("SELECT name FROM t WHERE id = ?1")
            .params(&[RowValues::Int(1)])
            .select()
            .await?;
        assert_eq!(
            rs.results[0].get("name").unwrap().as_text().unwrap(),
            "alice"
        );

        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

fn test_cases() -> Vec<TestCase> {
    let mut cases = vec![
        TestCase::Sqlite("file::memory:?cache=shared".to_string()),
        TestCase::Sqlite(unique_path("test_sqlite")),
    ];
    add_turso_cases(&mut cases);
    cases
}

#[cfg(feature = "turso")]
fn add_turso_cases(cases: &mut Vec<TestCase>) {
    cases.push(TestCase::Turso(":memory:".to_string()));
    cases.push(TestCase::Turso(unique_path("test_turso")));
}

#[cfg(not(feature = "turso"))]
fn add_turso_cases(_cases: &mut Vec<TestCase>) {}
