use sql_middleware::middleware::{
    ConversionMode, MiddlewarePoolConnection, QueryAndParams, RowValues,
};
use sql_middleware::postgres::{
    Params as PostgresParams, build_result_set as postgres_build_result_set,
};
use sql_middleware::sqlite::{Params as SqliteParams, build_result_set as sqlite_build_result_set};
use sql_middleware::{SqlMiddlewareDbError, convert_sql_params};

pub(super) async fn insert_and_assert(
    conn: &mut MiddlewarePoolConnection,
    parameterized_query: &str,
    count_query: &str,
    test_table: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let query_and_params = QueryAndParams {
        query: parameterized_query.to_string(),
        params: vec![RowValues::Int(990), RowValues::Text("name_990".to_string())],
    };
    insert_twice(conn, query_and_params, count_query.to_string()).await?;
    assert_duplicate_row(conn, test_table).await?;
    assert_total_count(conn, test_table).await
}

async fn insert_twice(
    conn: &mut MiddlewarePoolConnection,
    query_and_params: QueryAndParams,
    count_query: String,
) -> Result<(), SqlMiddlewareDbError> {
    match &mut *conn {
        MiddlewarePoolConnection::Postgres { client, .. } => {
            let tx = client.transaction().await?;
            let converted = PostgresParams::convert_for_batch(&query_and_params.params)?;
            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted, &tx).await?;
            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted, &tx).await?;
            tx.commit().await?;
            Ok(())
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql { .. } => {
            middleware_insert_twice(conn, &query_and_params).await
        }
        MiddlewarePoolConnection::Sqlite { .. } => {
            conn.with_blocking_sqlite(move |raw| {
                sqlite_insert_twice(raw, &query_and_params, &count_query)
            })
            .await?;
            Ok(())
        }
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { .. } => {
            middleware_insert_twice(conn, &query_and_params).await
        }
    }
}

async fn middleware_insert_twice(
    conn: &mut MiddlewarePoolConnection,
    query_and_params: &QueryAndParams,
) -> Result<(), SqlMiddlewareDbError> {
    conn.query(&query_and_params.query)
        .params(&query_and_params.params)
        .dml()
        .await?;
    conn.query(&query_and_params.query)
        .params(&query_and_params.params)
        .dml()
        .await?;
    Ok(())
}

fn sqlite_insert_twice(
    raw: &mut rusqlite::Connection,
    query_and_params: &QueryAndParams,
    count_query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let tx = raw
        .transaction()
        .map_err(|e| SqlMiddlewareDbError::Other(format!("sqlite tx3 start: {e}")))?;
    execute_sqlite_statement(&tx, query_and_params)?;
    assert_count_inside_tx(&tx, count_query, 401)?;
    execute_sqlite_statement(&tx, query_and_params)?;
    tx.commit()?;
    Ok(())
}

fn execute_sqlite_statement(
    tx: &rusqlite::Transaction<'_>,
    query_and_params: &QueryAndParams,
) -> Result<(), SqlMiddlewareDbError> {
    let converted =
        convert_sql_params::<SqliteParams>(&query_and_params.params, ConversionMode::Query)?;
    let mut stmt = tx.prepare(&query_and_params.query)?;
    sqlite_build_result_set(&mut stmt, converted.as_values())?;
    Ok(())
}

fn assert_count_inside_tx(
    tx: &rusqlite::Transaction<'_>,
    count_query: &str,
    expected: i64,
) -> Result<(), SqlMiddlewareDbError> {
    let query_and_params = QueryAndParams {
        query: count_query.to_string(),
        params: vec![],
    };
    let converted =
        convert_sql_params::<SqliteParams>(&query_and_params.params, ConversionMode::Query)?;
    let mut stmt = tx.prepare(&query_and_params.query)?;
    let result_set = sqlite_build_result_set(&mut stmt, converted.as_values())?;
    assert_eq!(result_set.results.len(), 1);
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        expected
    );
    Ok(())
}

async fn assert_duplicate_row(
    conn: &mut MiddlewarePoolConnection,
    test_table: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let query =
        format!("select count(*) as cnt,name from {test_table} where id = 990 group by name;");
    let result_set = conn.query(&query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        2
    );
    assert_eq!(
        result_set.results[0]
            .get("name")
            .unwrap()
            .as_text()
            .unwrap(),
        "name_990"
    );
    Ok(())
}

async fn assert_total_count(
    conn: &mut MiddlewarePoolConnection,
    test_table: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let query = format!("select count(*) as cnt from {test_table} ;");
    let result_set = conn.query(&query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        402
    );
    Ok(())
}
