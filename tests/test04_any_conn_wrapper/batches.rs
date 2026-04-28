use sql_middleware::middleware::{
    AnyConnWrapper, ConversionMode, DatabaseType, MiddlewarePoolConnection, RowValues,
};
use sql_middleware::postgres::Params as PostgresParams;
use sql_middleware::sqlite::Params as SqliteParams;
use sql_middleware::{SqlMiddlewareDbError, convert_sql_params};

pub(super) async fn insert_individual(
    conn: &mut MiddlewarePoolConnection,
    parameterized_query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    for param in params_for_range(0..100) {
        conn.query(parameterized_query).params(&param).dml().await?;
    }
    Ok(())
}

pub(super) async fn insert_backend_tx(
    conn: &mut MiddlewarePoolConnection,
    db_type: &DatabaseType,
    parameterized_query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let params = params_for_range(100..200);
    match db_type {
        DatabaseType::Postgres => insert_postgres(conn, parameterized_query, params).await,
        DatabaseType::Sqlite => insert_sqlite(conn, parameterized_query, params).await,
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => insert_via_middleware(conn, parameterized_query, params).await,
        #[cfg(feature = "turso")]
        DatabaseType::Turso => insert_via_middleware(conn, parameterized_query, params).await,
    }
}

pub(super) async fn insert_backend_tx_with_count_check(
    conn: &mut MiddlewarePoolConnection,
    db_type: &DatabaseType,
    parameterized_query: &str,
    count_query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let params = params_for_range(0..200);
    match db_type {
        DatabaseType::Postgres => insert_postgres(conn, parameterized_query, params).await,
        DatabaseType::Sqlite => {
            insert_sqlite_with_count_check(conn, parameterized_query, count_query, params).await
        }
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => insert_via_middleware(conn, parameterized_query, params).await,
        #[cfg(feature = "turso")]
        DatabaseType::Turso => insert_via_middleware(conn, parameterized_query, params).await,
    }
}

fn params_for_range(range: std::ops::Range<i64>) -> Vec<Vec<RowValues>> {
    range
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect()
}

async fn insert_postgres(
    conn: &mut MiddlewarePoolConnection,
    parameterized_query: &str,
    params: Vec<Vec<RowValues>>,
) -> Result<(), SqlMiddlewareDbError> {
    if let MiddlewarePoolConnection::Postgres {
        client: pg_handle, ..
    } = conn
    {
        let tx = pg_handle.transaction().await?;
        for param in params {
            let postgres_params = PostgresParams::convert(&param)?;
            tx.execute(parameterized_query, postgres_params.as_refs())
                .await?;
        }
        tx.commit().await?;
        Ok(())
    } else {
        Err(SqlMiddlewareDbError::Other(
            "expected postgres connection".to_string(),
        ))
    }
}

async fn insert_sqlite(
    conn: &mut MiddlewarePoolConnection,
    parameterized_query: &str,
    params: Vec<Vec<RowValues>>,
) -> Result<(), SqlMiddlewareDbError> {
    conn.interact_sync({
        let parameterized_query = parameterized_query.to_string();
        move |wrapper| match wrapper {
            AnyConnWrapper::Sqlite(sql_conn) => {
                let tx = sql_conn
                    .transaction()
                    .map_err(|e| SqlMiddlewareDbError::Other(format!("sqlite tx1 start: {e}")))?;
                for param in params {
                    let converted =
                        convert_sql_params::<SqliteParams>(&param, ConversionMode::Execute)?;
                    let refs = converted.as_refs();
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
    .await?
}

async fn insert_sqlite_with_count_check(
    conn: &mut MiddlewarePoolConnection,
    parameterized_query: &str,
    count_query: &str,
    params: Vec<Vec<RowValues>>,
) -> Result<(), SqlMiddlewareDbError> {
    conn.interact_sync({
        let parameterized_query = parameterized_query.to_string();
        let count_query = count_query.to_string();
        move |mut wrapper| {
            sqlite_tx_with_count_check(&mut wrapper, &parameterized_query, &count_query, params)
        }
    })
    .await?
}

fn sqlite_tx_with_count_check(
    wrapper: &mut AnyConnWrapper<'_>,
    parameterized_query: &str,
    count_query: &str,
    params: Vec<Vec<RowValues>>,
) -> Result<(), SqlMiddlewareDbError> {
    match wrapper {
        AnyConnWrapper::Sqlite(sql_conn) => {
            let tx = sql_conn
                .transaction()
                .map_err(|e| SqlMiddlewareDbError::Other(format!("sqlite tx2 start: {e}")))?;
            assert_sqlite_count(&tx, count_query, 200)?;
            let mut stmt = tx.prepare(parameterized_query)?;
            for param in params {
                let converted =
                    convert_sql_params::<SqliteParams>(&param, ConversionMode::Execute)?;
                let refs = converted.as_refs();
                stmt.execute(&refs[..])?;
            }
            drop(stmt);
            assert_sqlite_count(&tx, count_query, 400)?;
            tx.commit()?;
            Ok(())
        }
        _ => Err(SqlMiddlewareDbError::Other(
            "Unexpected database type".into(),
        )),
    }
}

fn assert_sqlite_count(
    tx: &rusqlite::Transaction<'_>,
    count_query: &str,
    expected: i32,
) -> Result<(), SqlMiddlewareDbError> {
    let mut stmt = tx.prepare(count_query)?;
    let mut res = stmt.query(rusqlite::params![])?;
    let count: i32 = if let Some(row) = res.next()? {
        row.get(0)?
    } else {
        0
    };
    assert_eq!(count, expected);
    Ok(())
}

async fn insert_via_middleware(
    conn: &mut MiddlewarePoolConnection,
    parameterized_query: &str,
    params: Vec<Vec<RowValues>>,
) -> Result<(), SqlMiddlewareDbError> {
    for param in params {
        conn.query(parameterized_query).params(&param).dml().await?;
    }
    Ok(())
}
