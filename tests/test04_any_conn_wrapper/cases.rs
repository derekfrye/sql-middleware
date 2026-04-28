#[cfg(feature = "mssql")]
use std::{fs, path::Path};

use sql_middleware::SqlMiddlewareDbError;
#[cfg(feature = "mssql")]
use sql_middleware::middleware::MssqlOptions;
use sql_middleware::middleware::{
    ConfigAndPool as ConfigAndPool2, DatabaseType, MiddlewarePoolConnection, PgConfig,
};

pub(super) enum TestCase {
    Sqlite(String),
    #[cfg(feature = "postgres")]
    Postgres(Box<PgConfig>),
    #[cfg(feature = "mssql")]
    Mssql(MssqlOptions),
    #[cfg(feature = "turso")]
    Turso(String),
}

pub(super) struct FileCleanup(Vec<String>);

impl Drop for FileCleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_file(format!("{p}-wal"));
            let _ = std::fs::remove_file(format!("{p}-shm"));
        }
    }
}

pub(super) fn assemble_test_cases() -> Result<Vec<TestCase>, Box<dyn std::error::Error>> {
    let mut test_cases = vec![
        TestCase::Sqlite("file::memory:?cache=shared".to_string()),
        TestCase::Sqlite(unique_path("test_sqlite")),
    ];
    add_postgres_case(&mut test_cases);
    add_mssql_case(&mut test_cases)?;
    add_turso_cases(&mut test_cases);
    Ok(test_cases)
}

#[cfg(feature = "postgres")]
fn add_postgres_case(test_cases: &mut Vec<TestCase>) {
    let mut cfg = PgConfig::new();
    cfg.dbname = Some("testing".to_string());
    cfg.host = Some("10.3.0.201".to_string());
    cfg.port = Some(5432);
    cfg.user = Some("testuser".to_string());
    cfg.password = Some(String::new());
    test_cases.push(TestCase::Postgres(Box::new(cfg)));
}

#[cfg(not(feature = "postgres"))]
fn add_postgres_case(_test_cases: &mut Vec<TestCase>) {}

#[cfg(feature = "mssql")]
fn add_mssql_case(test_cases: &mut Vec<TestCase>) -> Result<(), Box<dyn std::error::Error>> {
    let pwd = read_sql_server_password()?;
    test_cases.push(TestCase::Mssql(MssqlOptions::new(
        "10.3.0.202".to_string(),
        "testing".to_string(),
        "testlogin".to_string(),
        pwd,
        Some(1433),
        None,
    )));
    Ok(())
}

#[cfg(not(feature = "mssql"))]
fn add_mssql_case(_test_cases: &mut Vec<TestCase>) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(feature = "turso")]
fn add_turso_cases(test_cases: &mut Vec<TestCase>) {
    test_cases.push(TestCase::Turso(":memory:".to_string()));
    test_cases.push(TestCase::Turso(unique_path("test_turso")));
}

#[cfg(not(feature = "turso"))]
fn add_turso_cases(_test_cases: &mut Vec<TestCase>) {}

#[cfg(feature = "mssql")]
fn read_sql_server_password() -> Result<String, Box<dyn std::error::Error>> {
    let pwd_path = Path::new("tests/sql_server_pwd.txt");
    let pwd = fs::read_to_string(pwd_path)?;
    Ok(pwd.trim().to_string())
}

pub(super) async fn init_connection(
    test_case: TestCase,
) -> Result<(MiddlewarePoolConnection, DatabaseType, Option<FileCleanup>), Box<dyn std::error::Error>>
{
    let db_type = db_type_for_case(&test_case);
    let cleanup_guard = cleanup_for_case(&test_case);
    let conn = connection_for_case(test_case).await?;
    Ok((conn, db_type, cleanup_guard))
}

fn db_type_for_case(test_case: &TestCase) -> DatabaseType {
    match test_case {
        TestCase::Sqlite(_) => DatabaseType::Sqlite,
        #[cfg(feature = "turso")]
        TestCase::Turso(_) => DatabaseType::Turso,
        #[cfg(feature = "postgres")]
        TestCase::Postgres(_) => DatabaseType::Postgres,
        #[cfg(feature = "mssql")]
        TestCase::Mssql(_) => DatabaseType::Mssql,
    }
}

fn cleanup_for_case(test_case: &TestCase) -> Option<FileCleanup> {
    match test_case {
        TestCase::Sqlite(path) if path != "file::memory:?cache=shared" => Some(cleanup_file(path)),
        #[cfg(feature = "turso")]
        TestCase::Turso(path) if path != ":memory:" => Some(cleanup_file(path)),
        _ => None,
    }
}

fn cleanup_file(path: &str) -> FileCleanup {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));
    FileCleanup(vec![path.to_string()])
}

async fn connection_for_case(
    test_case: TestCase,
) -> Result<MiddlewarePoolConnection, Box<dyn std::error::Error>> {
    let conn = match test_case {
        TestCase::Sqlite(connection_string) => {
            ConfigAndPool2::sqlite_builder(connection_string)
                .build()
                .await?
                .get_connection()
                .await?
        }
        #[cfg(feature = "mssql")]
        TestCase::Mssql(opts) => {
            ConfigAndPool2::new_mssql(opts)
                .await?
                .get_connection()
                .await?
        }
        #[cfg(feature = "postgres")]
        TestCase::Postgres(cfg) => {
            ConfigAndPool2::postgres_builder((*cfg).clone())
                .build()
                .await?
                .get_connection()
                .await?
        }
        #[cfg(feature = "turso")]
        TestCase::Turso(connection_string) => {
            ConfigAndPool2::turso_builder(connection_string)
                .build()
                .await?
                .get_connection()
                .await?
        }
    };
    Ok(conn)
}

pub(super) async fn reset_backend(
    conn: &mut MiddlewarePoolConnection,
    db_type: &DatabaseType,
) -> Result<(), SqlMiddlewareDbError> {
    if db_type == &DatabaseType::Postgres {
        conn.execute_batch(
            r"
            DROP TABLE IF EXISTS eup_statistic CASCADE;
            DROP TABLE IF EXISTS event_user_player CASCADE;
            DROP TABLE IF EXISTS bettor CASCADE;
            DROP TABLE IF EXISTS golfer CASCADE;
            DROP TABLE IF EXISTS event CASCADE;
            DROP TABLE IF EXISTS test CASCADE;
            ",
        )
        .await?;
    }
    #[cfg(feature = "mssql")]
    if db_type == &DatabaseType::Mssql {
        conn.execute_batch(
            r"
            IF OBJECT_ID('dbo.eup_statistic', 'U') IS NOT NULL DROP TABLE dbo.eup_statistic;
            IF OBJECT_ID('dbo.event_user_player', 'U') IS NOT NULL DROP TABLE dbo.event_user_player;
            IF OBJECT_ID('dbo.bettor', 'U') IS NOT NULL DROP TABLE dbo.bettor;
            IF OBJECT_ID('dbo.golfer', 'U') IS NOT NULL DROP TABLE dbo.golfer;
            IF OBJECT_ID('dbo.event', 'U') IS NOT NULL DROP TABLE dbo.event;
            IF OBJECT_ID('dbo.test', 'U') IS NOT NULL DROP TABLE dbo.test;
            ",
        )
        .await?;
    }
    Ok(())
}

fn unique_path(prefix: &str) -> String {
    let pid = std::process::id();
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{pid}_{ns}.db")
}
