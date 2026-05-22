#[path = "test04_any_conn_wrapper/batches.rs"]
mod batches;
#[path = "test04_any_conn_wrapper/cases.rs"]
mod cases;
#[path = "test04_any_conn_wrapper/duplicate.rs"]
mod duplicate;
#[path = "test04_any_conn_wrapper/schema.rs"]
mod schema;

use sql_middleware::{
    SqlMiddlewareDbError,
    middleware::{DatabaseType, MiddlewarePoolConnection},
};
use tokio::runtime::Runtime;

#[test]
fn test4_trait() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = cases::assemble_test_cases()?;
    let rt = Runtime::new().unwrap();
    rt.block_on(async { run_test_cases(test_cases).await })
}

async fn run_test_cases(
    test_cases: Vec<cases::TestCase>,
) -> Result<(), Box<dyn std::error::Error>> {
    for test_case in test_cases {
        let (mut conn, db_type, _cleanup) = cases::init_connection(test_case).await?;
        cases::reset_backend(&mut conn, &db_type).await?;
        run_test_logic(&mut conn, db_type).await?;
    }

    Ok(())
}

async fn assert_count(
    conn: &mut MiddlewarePoolConnection,
    count_query: &str,
    expected: i64,
) -> Result<(), SqlMiddlewareDbError> {
    let result_set = conn.query(count_query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        expected
    );
    Ok(())
}

async fn run_test_logic(
    conn: &mut MiddlewarePoolConnection,
    db_type: DatabaseType,
) -> Result<(), SqlMiddlewareDbError> {
    schema::apply_schema(conn, &db_type).await?;

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

    batches::insert_individual(conn, &parameterized_query).await?;
    assert_count(conn, &count_query, 100).await?;
    assert_mssql_native_row_mapping(conn, &db_type, &test_table).await?;
    batches::insert_backend_tx(conn, &db_type, &parameterized_query).await?;
    assert_count(conn, &count_query, 200).await?;
    batches::insert_backend_tx_with_count_check(conn, &db_type, &parameterized_query, &count_query)
        .await?;
    let result_set = conn.query(&count_query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        400
    );

    duplicate::insert_and_assert(conn, &parameterized_query, &count_query, &test_table).await?;

    if db_type == DatabaseType::Postgres {
        conn.execute_batch(&format!("DROP TABLE IF EXISTS {test_table} CASCADE;"))
            .await?;
        cases::reset_backend(conn, &db_type).await?;
    }

    Ok(())
}

#[cfg(feature = "mssql")]
async fn assert_mssql_native_row_mapping(
    conn: &mut MiddlewarePoolConnection,
    db_type: &DatabaseType,
    test_table: &str,
) -> Result<(), SqlMiddlewareDbError> {
    if db_type != &DatabaseType::Mssql {
        return Ok(());
    }

    let MiddlewarePoolConnection::Mssql {
        conn: mssql_client, ..
    } = conn
    else {
        return Ok(());
    };

    let mut tx = sql_middleware::mssql::begin_transaction(mssql_client).await?;
    let prepared = tx.prepare(&format!("SELECT name FROM {test_table} WHERE id = @p1"))?;
    let mapped_name = tx
        .query_prepared_map_one(&prepared, &[sql_middleware::RowValues::Int(1)], |row| {
            let value = row
                .try_get::<&str, _>(0)?
                .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("name was NULL".into()))?;
            Ok(value.to_string())
        })
        .await?;
    assert_eq!(mapped_name, "name_1");

    let mapped_missing = tx
        .query_prepared_map_optional(&prepared, &[sql_middleware::RowValues::Int(-1)], |row| {
            let value = row
                .try_get::<&str, _>(0)?
                .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("name was NULL".into()))?;
            Ok(value.to_string())
        })
        .await?;
    assert!(mapped_missing.is_none());
    tx.commit().await?;

    Ok(())
}

#[cfg(not(feature = "mssql"))]
async fn assert_mssql_native_row_mapping(
    _conn: &mut MiddlewarePoolConnection,
    _db_type: &DatabaseType,
    _test_table: &str,
) -> Result<(), SqlMiddlewareDbError> {
    Ok(())
}
