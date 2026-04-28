#![cfg(any(feature = "sqlite", feature = "turso"))]
#[path = "test03_sqlite/assertions.rs"]
mod assertions;

use assertions::assert_selected_rows;
use sql_middleware::middleware::{ConfigAndPool, RowValues};
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
fn sqlite_and_turso_multiple_column_test_db2() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;

    let test_cases = test_cases();

    for case in test_cases {
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
            // Build pool
            let cap = match case {
                TestCase::Sqlite(path) => ConfigAndPool::sqlite_builder(path).build().await?,
                #[cfg(feature = "turso")]
                TestCase::Turso(path) => ConfigAndPool::turso_builder(path).build().await?,
            };
            let mut conn = cap.get_connection().await?;

            // Create table
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

            for (sql, params) in setup_queries.into_iter().zip(param_sets) {
                conn.query(sql).params(&params).dml().await?;
            }

            // Query a few rows
            let select_sql = "SELECT * from test where recid in ( ?1, ?2, ?3, ?4);";
            let select_params = [
                RowValues::Int(1),
                RowValues::Int(2),
                RowValues::Int(3),
                RowValues::Int(10),
            ];
            let res = conn
                .query(select_sql)
                .params(&select_params)
                .select()
                .await?;

            // we expect 4 rows
            assert_eq!(res.results.len(), 4);

            assert_selected_rows(&res);

            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
    }

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
