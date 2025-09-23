#![cfg(any(feature = "sqlite", feature = "turso"))]
use chrono::NaiveDateTime;
use serde_json::json;
use sql_middleware::middleware::{AsyncDatabaseExecutor, ConfigAndPool, MiddlewarePool, RowValues};
use tokio::runtime::Runtime;

enum TestCase {
    Sqlite(String),
    #[cfg(feature = "turso")]
    Turso(String),
}

#[test]
fn sqlite_and_turso_multiple_column_test_db2() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;

    fn unique_path(prefix: &str) -> String {
        let pid = std::process::id();
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{}_{}_{}.db", prefix, pid, ns)
    }

    let  test_cases = vec![
        TestCase::Sqlite("file::memory:?cache=shared".to_string()),
        TestCase::Sqlite(unique_path("test_sqlite")),
    ];

    #[cfg(feature = "turso")]
    {
        test_cases.push(TestCase::Turso(":memory:".to_string()));
        test_cases.push(TestCase::Turso(unique_path("test_turso")));
    }

    // Drop guard to clean up file-backed DBs even on failure
    struct FileCleanup(Vec<String>);
    impl Drop for FileCleanup {
        fn drop(&mut self) {
            for p in &self.0 {
                let _ = std::fs::remove_file(p);
                let _ = std::fs::remove_file(format!("{}-wal", p));
                let _ = std::fs::remove_file(format!("{}-shm", p));
            }
        }
    }

    for case in test_cases {
        let _cleanup_guard = match &case {
            TestCase::Sqlite(path) if path != "file::memory:?cache=shared" => {
                let _ = std::fs::remove_file(path);
                Some(FileCleanup(vec![path.clone()]))
            }
            #[cfg(feature = "turso")]
            TestCase::Turso(path) if path != ":memory:" => {
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(format!("{}-wal", path));
                let _ = std::fs::remove_file(format!("{}-shm", path));
                Some(FileCleanup(vec![path.clone()]))
            }
            _ => None,
        };

        rt.block_on(async {
            // Build pool
            let cap = match case {
                TestCase::Sqlite(path) => ConfigAndPool::new_sqlite(path).await?,
                #[cfg(feature = "turso")]
                TestCase::Turso(path) => ConfigAndPool::new_turso(path).await?,
            };
            let pool = cap.pool.get().await?;
            let mut conn = MiddlewarePool::get_connection(pool).await?;

            // Create table
            let ddl = r#"
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
            "#;
            conn.execute_batch(ddl).await?;

            // Apply setup inserts (parameterized)
            let setup_queries = vec![
                include_str!("../tests/sqlite/test3/test3_01_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_02_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_03_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_04_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_05_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_06_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_07_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_08_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_09_setup.sql"),
                include_str!("../tests/sqlite/test3/test3_10_setup.sql"),
            ];
            let param_sets: Vec<Vec<RowValues>> = vec![
                vec![RowValues::Int(1)],
                vec![RowValues::Int(2)],
                vec![RowValues::Int(3)],
                vec![RowValues::Int(4)],
                vec![RowValues::Int(5)],
                vec![RowValues::Int(6)],
                vec![RowValues::Int(7)],
                vec![RowValues::Int(8)],
                vec![RowValues::Int(9)],
                vec![
                    RowValues::Int(100),
                    RowValues::Text("Juliet".to_string()),
                    RowValues::Float(100.75),
                ],
            ];

            for (sql, params) in setup_queries.into_iter().zip(param_sets.into_iter()) {
                conn.execute_dml(sql, &params).await?;
            }

            // Query a few rows
            let select_sql = "SELECT * from test where recid in ( ?1, ?2, ?3, ?4);";
            let select_params = [
                RowValues::Int(1),
                RowValues::Int(2),
                RowValues::Int(3),
                RowValues::Int(10),
            ];
            let res = conn.execute_select(select_sql, &select_params).await?;

            // we expect 4 rows
            assert_eq!(res.results.len(), 4);

            // row 1
            assert_eq!(*res.results[0].get("recid").unwrap().as_int().unwrap(), 1);
            assert_eq!(*res.results[0].get("a").unwrap().as_int().unwrap(), 1);
            assert_eq!(res.results[0].get("b").unwrap().as_text().unwrap(), "Alpha");
            assert_eq!(
                res.results[0].get("c").unwrap().as_timestamp().unwrap(),
                NaiveDateTime::parse_from_str("2024-01-01 08:00:01", "%Y-%m-%d %H:%M:%S").unwrap()
            );
            assert_eq!(res.results[0].get("d").unwrap().as_float().unwrap(), 10.5);
            assert!(*res.results[0].get("e").unwrap().as_bool().unwrap());
            assert_eq!(
                res.results[0].get("f").unwrap().as_blob().unwrap(),
                b"Blob12"
            );
            // JSON as text
            assert_eq!(
                json!(res.results[0].get("g").unwrap().as_text().unwrap()),
                json!(r#"{"name": "Alice", "age": 30}"#)
            );

            // row 3
            assert_eq!(*res.results[2].get("recid").unwrap().as_int().unwrap(), 3);
            assert_eq!(*res.results[2].get("a").unwrap().as_int().unwrap(), 3);
            assert_eq!(res.results[2].get("b").unwrap().as_text().unwrap(), "Charlie");
            assert_eq!(
                res.results[2].get("c").unwrap().as_timestamp().unwrap(),
                NaiveDateTime::parse_from_str("2024-01-03 10:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
            );
            assert_eq!(res.results[2].get("d").unwrap().as_float().unwrap(), 30.25);
            assert!(*res.results[2].get("e").unwrap().as_bool().unwrap());

            // param row (Juliet)
            assert_eq!(*res.results[3].get("a").unwrap().as_int().unwrap(), 100);
            assert_eq!(res.results[3].get("d").unwrap().as_float().unwrap(), 100.75);
            assert_eq!(res.results[3].get("f").unwrap().as_blob().unwrap(), b"Blob11");

            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
    }

    Ok(())
}
