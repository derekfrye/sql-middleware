#[cfg(feature = "mssql")]
use std::{fs, path::Path};

#[cfg(feature = "mssql")]
use sql_middleware::middleware::MssqlOptions;
use sql_middleware::postgres::{
    Params as PostgresParams, build_result_set as postgres_build_result_set,
};
use sql_middleware::sqlite::{Params as SqliteParams, build_result_set as sqlite_build_result_set};
use sql_middleware::{
    SqlMiddlewareDbError, convert_sql_params,
    middleware::{
        AnyConnWrapper, ConfigAndPool as ConfigAndPool2, ConversionMode, DatabaseType,
        MiddlewarePoolConnection, PgConfig, QueryAndParams, RowValues,
    },
};
use tokio::runtime::Runtime;

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

#[cfg(feature = "mssql")]
fn read_sql_server_password() -> Result<String, Box<dyn std::error::Error>> {
    let pwd_path = Path::new("tests/sql_server_pwd.txt");
    let pwd = fs::read_to_string(pwd_path)?;
    Ok(pwd.trim().to_string())
}

#[test]
fn test4_trait() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = assemble_test_cases()?;
    let rt = Runtime::new().unwrap();
    rt.block_on(async { run_test_cases(test_cases).await })
}

#[allow(unused_mut)]
fn assemble_test_cases() -> Result<Vec<TestCase>, Box<dyn std::error::Error>> {
    #[allow(unused_mut)]
    let mut test_cases = vec![
        TestCase::Sqlite("file::memory:?cache=shared".to_string()),
        TestCase::Sqlite(unique_path("test_sqlite")),
    ];
    #[cfg(feature = "postgres")]
    {
        let mut cfg = PgConfig::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string());
        cfg.password = Some(String::new());
        test_cases.push(TestCase::Postgres(Box::new(cfg)));
    }
    #[cfg(feature = "mssql")]
    {
        let pwd = read_sql_server_password()?;
        test_cases.push(TestCase::Mssql(MssqlOptions::new(
            "10.3.0.202".to_string(),
            "testing".to_string(),
            "testlogin".to_string(),
            pwd,
            Some(1433),
            None,
        )));
    }
    #[cfg(feature = "turso")]
    {
        test_cases.push(TestCase::Turso(":memory:".to_string()));
        test_cases.push(TestCase::Turso(unique_path("test_turso")));
    }

    Ok(test_cases)
}

async fn run_test_cases(test_cases: Vec<TestCase>) -> Result<(), Box<dyn std::error::Error>> {
    for test_case in test_cases {
        let (mut conn, db_type, _cleanup) = init_connection(test_case).await?;
        reset_backend(&mut conn, &db_type).await?;
        run_test_logic(&mut conn, db_type).await?;
    }

    Ok(())
}

async fn init_connection(
    test_case: TestCase,
) -> Result<(MiddlewarePoolConnection, DatabaseType, Option<FileCleanup>), Box<dyn std::error::Error>>
{
    let db_type: DatabaseType;
    let cleanup_guard = match &test_case {
        TestCase::Sqlite(connection_string) => {
            db_type = DatabaseType::Sqlite;
            if connection_string == "file::memory:?cache=shared" {
                None
            } else {
                let _ = std::fs::remove_file(connection_string);
                Some(FileCleanup(vec![connection_string.clone()]))
            }
        }
        #[cfg(feature = "turso")]
        TestCase::Turso(connection_string) => {
            db_type = DatabaseType::Turso;
            if connection_string == ":memory:" {
                None
            } else {
                let _ = std::fs::remove_file(connection_string);
                let _ = std::fs::remove_file(format!("{connection_string}-wal"));
                let _ = std::fs::remove_file(format!("{connection_string}-shm"));
                Some(FileCleanup(vec![connection_string.clone()]))
            }
        }
        #[cfg(feature = "postgres")]
        TestCase::Postgres(_) => {
            db_type = DatabaseType::Postgres;
            None
        }
        #[cfg(feature = "mssql")]
        TestCase::Mssql(_) => {
            db_type = DatabaseType::Mssql;
            None
        }
    };

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

    Ok((conn, db_type, cleanup_guard))
}

async fn reset_backend(
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

enum TestCase {
    Sqlite(String),
    #[cfg(feature = "postgres")]
    Postgres(Box<PgConfig>),
    #[cfg(feature = "mssql")]
    Mssql(MssqlOptions),
    #[cfg(feature = "turso")]
    Turso(String),
}

#[allow(clippy::too_many_lines)]
async fn run_test_logic(
    conn: &mut MiddlewarePoolConnection,
    db_type: DatabaseType,
) -> Result<(), SqlMiddlewareDbError> {
    // Define the DDL statements
    let ddl = match db_type {
        DatabaseType::Postgres => vec![
            include_str!("../tests/postgres/test4/00_event.sql"),
            // include_str!("../src/admin/model/sql/schema/sqlite/01_golfstatistic.sql"),
            include_str!("../tests/postgres/test4/02_golfer.sql"),
            include_str!("../tests/postgres/test4/03_bettor.sql"),
            include_str!("../tests/postgres/test4/04_event_user_player.sql"),
            include_str!("../tests/postgres/test4/05_eup_statistic.sql"),
        ],
        DatabaseType::Sqlite => vec![
            include_str!("../tests/sqlite/test4/00_event.sql"),
            // include_str!("../src/admin/model/sql/schema/sqlite/01_golfstatistic.sql"),
            include_str!("../tests/sqlite/test4/02_golfer.sql"),
            include_str!("../tests/sqlite/test4/03_bettor.sql"),
            include_str!("../tests/sqlite/test4/04_event_user_player.sql"),
            include_str!("../tests/sqlite/test4/05_eup_statistic.sql"),
        ],
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => vec![
            r"
IF OBJECT_ID('dbo.event', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.event (
        event_id INT IDENTITY(1,1) PRIMARY KEY,
        espn_id INT NOT NULL UNIQUE,
        year INT NOT NULL,
        name NVARCHAR(255) NOT NULL,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()
    );
END;
",
            r"
IF OBJECT_ID('dbo.golfer', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.golfer (
        golfer_id INT IDENTITY(1,1) PRIMARY KEY,
        espn_id INT NOT NULL UNIQUE,
        name NVARCHAR(255) NOT NULL UNIQUE,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()
    );
END;
",
            r"
IF OBJECT_ID('dbo.bettor', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.bettor (
        user_id INT IDENTITY(1,1) PRIMARY KEY,
        name NVARCHAR(255) NOT NULL,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()
    );
END;
",
            r"
IF OBJECT_ID('dbo.event_user_player', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.event_user_player (
        eup_id INT IDENTITY(1,1) PRIMARY KEY,
        event_id INT NOT NULL REFERENCES dbo.event(event_id),
        user_id INT NOT NULL REFERENCES dbo.bettor(user_id),
        golfer_id INT NOT NULL REFERENCES dbo.golfer(golfer_id),
        last_refresh_ts DATETIME2 NULL,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
        CONSTRAINT uq_event_user_player UNIQUE (event_id, user_id, golfer_id)
    );
END;
",
            r"
IF OBJECT_ID('dbo.eup_statistic', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.eup_statistic (
        eup_stat_id INT IDENTITY(1,1) PRIMARY KEY,
        event_espn_id INT NOT NULL REFERENCES dbo.event(espn_id),
        golfer_espn_id INT NOT NULL REFERENCES dbo.golfer(espn_id),
        eup_id INT NOT NULL REFERENCES dbo.event_user_player(eup_id),
        grp INT NOT NULL,
        rounds NVARCHAR(MAX) NOT NULL,
        round_scores NVARCHAR(MAX) NOT NULL,
        tee_times NVARCHAR(MAX) NOT NULL,
        holes_completed_by_round NVARCHAR(MAX) NOT NULL,
        line_scores NVARCHAR(MAX) NOT NULL,
        total_score INT NOT NULL,
        upd_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
        CONSTRAINT uq_eup_stat UNIQUE (golfer_espn_id, eup_id)
    );
END;
",
        ],
        #[cfg(feature = "turso")]
        DatabaseType::Turso => vec![
            include_str!("../tests/turso/test4/00_event.sql"),
            include_str!("../tests/turso/test4/02_golfer.sql"),
            include_str!("../tests/turso/test4/03_bettor.sql"),
        ],
    };

    #[cfg(feature = "turso")]
    if db_type == DatabaseType::Turso {
        for (idx, stmt) in ddl.iter().enumerate() {
            conn.execute_batch(stmt).await.map_err(|error| {
                let part = idx + 1;
                SqlMiddlewareDbError::ExecutionError(format!(
                    "Turso DDL failure on part {part}: {error}"
                ))
            })?;
        }
    } else {
        let ddl_query = ddl.join("\n");
        conn.execute_batch(&ddl_query).await?;
    }
    #[cfg(not(feature = "turso"))]
    {
        let ddl_query = ddl.join("\n");
        conn.execute_batch(&ddl_query).await?;
    }

    // Define the setup queries
    let setup_queries = match db_type {
        DatabaseType::Postgres | DatabaseType::Sqlite => include_str!("test04.sql"),
        #[cfg(feature = "turso")]
        DatabaseType::Turso => include_str!("../tests/turso/test4/setup.sql"),
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => include_str!("test04.sql"),
    };
    conn.execute_batch(setup_queries).await?;

    let test_table = format!(
        "test04_anyconn_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let test_tbl_query = match db_type {
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => {
            format!("CREATE TABLE {test_table} (id BIGINT, name NVARCHAR(255));")
        }
        _ => format!("CREATE TABLE {test_table} (id bigint, name text);"),
    };
    conn.execute_batch(&test_tbl_query).await?;

    if db_type == DatabaseType::Sqlite {
        // Make sure earlier setup batches didn't leave us mid-transaction before starting explicit work.
        conn.with_blocking_sqlite(|raw| {
            if !raw.is_autocommit() {
                raw.execute_batch("COMMIT")?;
            }
            Ok::<_, SqlMiddlewareDbError>(())
        })
        .await?;
    }

    let parameterized_query = match db_type {
        DatabaseType::Postgres => format!("INSERT INTO {test_table} (id, name) VALUES ($1, $2);"),
        DatabaseType::Sqlite => format!("INSERT INTO {test_table} (id, name) VALUES (?1, ?2);"),
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => format!("INSERT INTO {test_table} (id, name) VALUES (@p1, @p2);"),
        #[cfg(feature = "turso")]
        DatabaseType::Turso => format!("INSERT INTO {test_table} (id, name) VALUES (?1, ?2);"),
    };
    let count_query = format!("select count(*) as cnt from {test_table};");

    // generate 100 params
    let params: Vec<Vec<RowValues>> = (0..100)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect();
    // dbg!(&params);

    // conn.execute_dml(parameterized_query, &params[0]).await?;

    // lets first run this through 100 transactions, yikes
    for param in params {
        // println!("param: {:?}", param);
        conn.query(&parameterized_query).params(&param).dml().await?;
    }

    let result_set = conn.query(&count_query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        100
    );

    // generate 100 more params
    let params: Vec<Vec<RowValues>> = (100..200)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect();

    // now let's be a little smarter and write our own loop to exec a 100 inserts

    match db_type {
        DatabaseType::Postgres => {
            if let MiddlewarePoolConnection::Postgres {
                client: pg_handle, ..
            } = conn
            {
                let tx = pg_handle.transaction().await?;
                for param in params {
                    let postgres_params = PostgresParams::convert(&param)?;
                    tx.execute(&parameterized_query, postgres_params.as_refs())
                        .await?;
                }
                tx.commit().await?;
            } else {
                // or return an error if we expect only Postgres here
                unimplemented!();
            }
        }
        DatabaseType::Sqlite => {
            let res = conn
                .interact_sync({
                    let parameterized_query = parameterized_query.clone();
                    let params = params.clone();
                    move |wrapper| match wrapper {
                        AnyConnWrapper::Sqlite(sql_conn) => {
                            let tx = sql_conn.transaction().map_err(|e| {
                                SqlMiddlewareDbError::Other(format!("sqlite tx1 start: {e}"))
                            })?;
                            for param in params {
                                let converted_params = convert_sql_params::<SqliteParams>(
                                    &param,
                                    ConversionMode::Execute,
                                )?;
                                let refs = converted_params.as_refs();
                                tx.execute(&parameterized_query, &refs[..])?;
                            }
                            tx.commit()?;
                            Ok(())
                        }
                        _ => Err(SqlMiddlewareDbError::Other(
                            "Unexpected database type".into(),
                        )),
                    }
                })
                .await?;
            res?;
        }
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => {
            // For this test, skip the MSSQL implementation
            // Simply insert the data using the middleware connection
            for param in params {
                conn.query(&parameterized_query).params(&param).dml().await?;
            }
        }
        #[cfg(feature = "turso")]
        DatabaseType::Turso => {
            for param in params {
                conn.query(&parameterized_query).params(&param).dml().await?;
            }
        }
    }

    let result_set = conn.query(&count_query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        200
    );

    // generate 200 more params
    let params: Vec<Vec<RowValues>> = (0..200)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect();

    // now let's be a little smarter and write our own loop to exec inserts
    // yes, this isn't as quick as bulk loading, which isn't implemented yet

    match db_type {
        DatabaseType::Postgres => {
            if let MiddlewarePoolConnection::Postgres {
                client: pg_handle, ..
            } = conn
            {
                let tx = pg_handle.transaction().await?;
                for param in params {
                    let postgres_params = PostgresParams::convert(&param)?;
                    tx.execute(&parameterized_query, postgres_params.as_refs())
                        .await?;
                }
                tx.commit().await?;
            } else {
                // or return an error if we expect only Postgres here
                unimplemented!();
            }
        }
        DatabaseType::Sqlite => {
            let count_query_for_tx2 = count_query.clone();
            let res = conn
                .interact_sync({
                    let parameterized_query = parameterized_query.clone();
                    let count_query = count_query_for_tx2;
                    let params = params.clone();
                    move |wrapper| {
                        match wrapper {
                            AnyConnWrapper::Sqlite(sql_conn) => {
                                let tx = sql_conn.transaction().map_err(|e| {
                                    SqlMiddlewareDbError::Other(format!("sqlite tx2 start: {e}"))
                                })?;
                                {
                                    let mut stmt = tx.prepare(&count_query)?;
                                    let mut res = stmt.query(rusqlite::params![])?;
                                    // let cnt: i64 = res.next().unwrap().get(0)?;
                                    let x: i32 = if let Some(row) = res.next()? {
                                        row.get(0)?
                                    } else {
                                        0
                                    };
                                    assert_eq!(x, 200);
                                }
                                {
                                    let mut stmt = tx.prepare(&parameterized_query)?;
                                    for param in params {
                                        let converted_params = convert_sql_params::<SqliteParams>(
                                            &param,
                                            ConversionMode::Execute,
                                        )?;
                                        let refs = converted_params.as_refs();
                                        stmt.execute(&refs[..])?;
                                    }
                                }
                                {
                                    let mut stmt = tx.prepare(&count_query)?;
                                    let mut res = stmt.query(rusqlite::params![])?;
                                    // let cnt: i64 = res.next().unwrap().get(0)?;
                                    let x: i32 = if let Some(row) = res.next()? {
                                        row.get(0)?
                                    } else {
                                        0
                                    };
                                    assert_eq!(x, 400);
                                }
                                tx.commit()?;
                                Ok(())
                            }
                            _ => Err(SqlMiddlewareDbError::Other(
                                "Unexpected database type".into(),
                            )),
                        }
                    }
                })
                .await?;
            res?;
        }
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => {
            // For MS SQL, insert data using the middleware connection
            for param in params {
                conn.query(&parameterized_query).params(&param).dml().await?;
            }
        }
        #[cfg(feature = "turso")]
        DatabaseType::Turso => {
            for param in params {
                conn.query(&parameterized_query).params(&param).dml().await?;
            }
        }
    }

    let result_set = conn.query(&count_query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        400
    );

    // let's test a common pattern in rusty-golf
    // generate 1 more param
    let params: Vec<RowValues> = vec![RowValues::Int(990), RowValues::Text("name_990".to_string())];

    let query_and_params = QueryAndParams {
        query: parameterized_query.to_string(),
        params,
    };

    (match &mut *conn {
        MiddlewarePoolConnection::Postgres { client: xx, .. } => {
            let tx = xx.transaction().await?;
            let converted_params = PostgresParams::convert_for_batch(&query_and_params.params)?;

            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted_params, &tx).await?;

            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted_params, &tx).await?;

            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql { .. } => {
            // For MSSQL, execute twice to match duplicate insert expectation
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
        MiddlewarePoolConnection::Sqlite { .. } => {
            Ok(conn
                .with_blocking_sqlite(move |xxx| {
                    let tx = xxx.transaction().map_err(|e| {
                        SqlMiddlewareDbError::Other(format!("sqlite tx3 start: {e}"))
                    })?;
                    {
                        let converted_params = convert_sql_params::<SqliteParams>(
                            &query_and_params.params,
                            ConversionMode::Query,
                        )?;
                        let mut stmt = tx.prepare(&query_and_params.query)?;
                        sqlite_build_result_set(&mut stmt, converted_params.as_values())?;
                    }
                    // should be able to read that val even tho we're not done w tx
                    {
                        let query_and_params_vec = QueryAndParams {
                            query: count_query.clone(),
                            params: vec![],
                        };
                        let converted_params = convert_sql_params::<SqliteParams>(
                            &query_and_params_vec.params,
                            ConversionMode::Query,
                        )?;
                        let mut stmt = tx.prepare(&query_and_params_vec.query)?;
                        let result_set =
                            sqlite_build_result_set(&mut stmt, converted_params.as_values())?;
                        assert_eq!(result_set.results.len(), 1);
                        assert_eq!(
                            *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
                            401
                        );
                    }
                    {
                        let converted_params = convert_sql_params::<SqliteParams>(
                            &query_and_params.params,
                            ConversionMode::Query,
                        )?;
                        let mut stmt = tx.prepare(&query_and_params.query)?;
                        sqlite_build_result_set(&mut stmt, converted_params.as_values())?;
                    }
                    tx.commit()?;
                    Ok::<_, SqlMiddlewareDbError>(result_set)
                })
                .await?)
        }

        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { .. } => {
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
    })?;

    // println!("dbdriver: {:?}, res: {:?}", db_type, res);

    // make sure to match the val from above
    let query = format!(
        "select count(*) as cnt,name from {test_table} where id = 990 group by name;"
    );
    let result_set = conn.query(&query).select().await?;
    // theres two in here now, we inserted same val 2x above
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        2
    );
    assert_eq!(
        *result_set.results[0]
            .get("name")
            .unwrap()
            .as_text()
            .unwrap(),
        *"name_990"
    );

    let query = format!("select count(*) as cnt from {test_table} ;");
    let result_set = conn.query(&query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        402
    );
    // assert_eq!(*result_set.results[0].get("name").unwrap().as_text().unwrap(), *"name_990");

    // assert_eq!(res.rows_affected, 1);

    Ok(())
}
