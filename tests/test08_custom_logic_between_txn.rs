#![cfg(any(feature = "sqlite", feature = "postgres", feature = "libsql", feature = "turso"))]

use std::env;

use sql_middleware::middleware::{
    ConfigAndPool, MiddlewarePoolConnection, PlaceholderStyle, RowValues, SqlMiddlewareDbError,
    translate_placeholders,
};
use tokio::runtime::Runtime;

#[cfg(feature = "turso")]
use sql_middleware::turso::{
    begin_transaction as begin_turso_tx, Prepared as TursoPrepared, Tx as TursoTx,
};
#[cfg(feature = "postgres")]
use sql_middleware::postgres::{
    begin_transaction as begin_postgres_tx, Prepared as PostgresPrepared, Tx as PostgresTx,
    PostgresOptions,
};
#[cfg(feature = "typed-postgres")]
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};
#[cfg(feature = "sqlite")]
use sql_middleware::sqlite::{
    begin_transaction as begin_sqlite_tx, Prepared as SqlitePrepared, Tx as SqliteTx,
};
#[cfg(feature = "libsql")]
use sql_middleware::libsql::{
    begin_transaction as begin_libsql_tx, Prepared as LibsqlPrepared, Tx as LibsqlTx,
};

#[cfg(feature = "postgres")]
fn postgres_config() -> deadpool_postgres::Config {
    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some("testing".to_string());
    cfg.host = Some("10.3.0.201".to_string());
    cfg.port = Some(5432);
    cfg.user = Some("testuser".to_string());
    // Trust auth in CI; allow override when a password is required.
    cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());
    cfg
}

enum BackendTx<'conn> {
    #[cfg(feature = "turso")]
    Turso(TursoTx<'conn>),
    #[cfg(feature = "postgres")]
    Postgres(PostgresTx<'conn>),
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteTx),
    #[cfg(feature = "libsql")]
    Libsql(LibsqlTx<'conn>),
}

enum PreparedStmt {
    #[cfg(feature = "turso")]
    Turso(TursoPrepared),
    #[cfg(feature = "postgres")]
    Postgres(PostgresPrepared),
    #[cfg(feature = "sqlite")]
    Sqlite(SqlitePrepared),
    #[cfg(feature = "libsql")]
    Libsql(LibsqlPrepared),
}

impl<'conn> BackendTx<'conn> {
    async fn commit(self) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "turso")]
            BackendTx::Turso(tx) => tx.commit().await,
            #[cfg(feature = "postgres")]
            BackendTx::Postgres(tx) => tx.commit().await,
            #[cfg(feature = "sqlite")]
            BackendTx::Sqlite(tx) => tx.commit().await,
            #[cfg(feature = "libsql")]
            BackendTx::Libsql(tx) => tx.commit().await,
        }
    }

    async fn rollback(self) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "turso")]
            BackendTx::Turso(tx) => tx.rollback().await,
            #[cfg(feature = "postgres")]
            BackendTx::Postgres(tx) => tx.rollback().await,
            #[cfg(feature = "sqlite")]
            BackendTx::Sqlite(tx) => tx.rollback().await,
            #[cfg(feature = "libsql")]
            BackendTx::Libsql(tx) => tx.rollback().await,
        }
    }
}

impl PreparedStmt {
    async fn execute_prepared(
        &mut self,
        tx: &BackendTx<'_>,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        match (tx, self) {
            #[cfg(feature = "turso")]
            (BackendTx::Turso(tx), PreparedStmt::Turso(stmt)) => {
                tx.execute_prepared(stmt, params).await
            }
            #[cfg(feature = "postgres")]
            (BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt)) => {
                tx.execute_prepared(stmt, params).await
            }
            #[cfg(feature = "sqlite")]
            (BackendTx::Sqlite(tx), PreparedStmt::Sqlite(stmt)) => {
                tx.execute_prepared(stmt, params).await
            }
            #[cfg(feature = "libsql")]
            (BackendTx::Libsql(tx), PreparedStmt::Libsql(stmt)) => {
                tx.execute_prepared(stmt, params).await
            }
            _ => unreachable!("transaction and prepared variants should align"),
        }
    }
}

async fn run_execute_with_finalize<'conn>(
    tx: BackendTx<'conn>,
    mut stmt: PreparedStmt,
    params: Vec<RowValues>,
) -> Result<usize, SqlMiddlewareDbError> {
    let result = stmt.execute_prepared(&tx, &params).await;
    match result {
        Ok(rows) => {
            tx.commit().await?;
            Ok(rows)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}

async fn prepare_backend_tx_and_stmt<'conn>(
    conn: &'conn mut MiddlewarePoolConnection,
    base_query: &str,
) -> Result<(BackendTx<'conn>, PreparedStmt), SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { conn, .. } => {
            let tx = begin_turso_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref()).await?;
            Ok((BackendTx::Turso(tx), PreparedStmt::Turso(stmt)))
        }
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres { client, .. } => {
            let tx = begin_postgres_tx(client).await?;
            let stmt = tx.prepare(base_query).await?;
            Ok((BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt)))
        }
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql { conn, .. } => {
            let tx = begin_libsql_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref())?;
            Ok((BackendTx::Libsql(tx), PreparedStmt::Libsql(stmt)))
        }
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { conn, .. } => {
            let tx = begin_sqlite_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref())?;
            Ok((BackendTx::Sqlite(tx), PreparedStmt::Sqlite(stmt)))
        }
        _ => Err(SqlMiddlewareDbError::Unimplemented(
            "expected Turso, Postgres, SQLite, or LibSQL connection".to_string(),
        )),
    }
}

async fn run_roundtrip(
    conn: &mut MiddlewarePoolConnection,
) -> Result<(), SqlMiddlewareDbError> {
    // Shared query authored once; translated for SQLite-family backends.
    let insert_query = "INSERT INTO custom_logic_txn (id, note) VALUES ($1, $2)";

    // Success path should commit.
    {
        let (tx, stmt) = prepare_backend_tx_and_stmt(conn, insert_query).await?;
        run_execute_with_finalize(
            tx,
            stmt,
            vec![RowValues::Int(1), RowValues::Text("ok".into())],
        )
        .await?;
    }

    // Duplicate insert should roll back and propagate the error.
    {
        let (tx, stmt) = prepare_backend_tx_and_stmt(conn, insert_query).await?;
        let res = run_execute_with_finalize(
            tx,
            stmt,
            vec![RowValues::Int(1), RowValues::Text("dup".into())],
        )
        .await;
        assert!(res.is_err(), "expected duplicate key to fail");
    }

    // Verify only the committed row exists.
    let rs = conn
        .query("SELECT COUNT(*) AS cnt FROM custom_logic_txn")
        .select()
        .await?;
    let count = *rs.results[0].get("cnt").unwrap().as_int().unwrap();
    assert_eq!(count, 1);
    Ok(())
}

#[cfg(all(feature = "typed-postgres", feature = "postgres"))]
async fn run_typed_pg_roundtrip(mut conn: PgConnection<PgIdle>) -> Result<(), SqlMiddlewareDbError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS custom_logic_txn;
         CREATE TABLE custom_logic_txn (id BIGINT PRIMARY KEY, note TEXT);",
    )
    .await?;

    // Success path should commit.
    {
        let mut tx = conn.begin().await?;
        let rows = tx
            .dml(
                "INSERT INTO custom_logic_txn (id, note) VALUES ($1, $2)",
                &[RowValues::Int(1), RowValues::Text("ok".into())],
            )
            .await?;
        assert_eq!(rows, 1);
        conn = tx.commit().await?;
    }

    // Duplicate insert should roll back and propagate the error.
    {
        let mut tx = conn.begin().await?;
        let res = tx
            .dml(
                "INSERT INTO custom_logic_txn (id, note) VALUES ($1, $2)",
                &[RowValues::Int(1), RowValues::Text("dup".into())],
            )
            .await;
        assert!(res.is_err(), "expected duplicate key to fail");
        conn = tx.rollback().await?;
    }

    // Verify only the committed row exists.
    let rs = conn
        .select("SELECT COUNT(*) AS cnt FROM custom_logic_txn", &[])
        .await?;
    let count = *rs.results[0].get("cnt").unwrap().as_int().unwrap();
    assert_eq!(count, 1);

    conn.execute_batch("DROP TABLE IF EXISTS custom_logic_txn;")
        .await?;
    Ok(())
}

#[test]
fn custom_logic_between_transactions_across_backends() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    rt.block_on(async {
        // SQLite (always available in default feature set)
        #[cfg(feature = "sqlite")]
        {
            let cap = ConfigAndPool::sqlite_builder("file::memory:?cache=shared".to_string())
                .build()
                .await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS custom_logic_txn (id INTEGER PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            println!("sqlite backend run successful");
        }

        // Postgres using the same auth flow as test06_*.
        #[cfg(feature = "postgres")]
        {
            let cfg = postgres_config();
            let cap = ConfigAndPool::new_postgres(PostgresOptions::new(cfg)).await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "DROP TABLE IF EXISTS custom_logic_txn;
                 CREATE TABLE custom_logic_txn (id BIGINT PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            conn.execute_batch("DROP TABLE IF EXISTS custom_logic_txn;")
                .await?;
            println!("postgres backend run successful");
        }

        // Typed Postgres mirror of the same flow.
        #[cfg(all(feature = "typed-postgres", feature = "postgres"))]
        {
            let cfg = postgres_config();
            let mut pg_cfg = tokio_postgres::Config::new();
            if let Some(user) = cfg.user.as_deref() {
                pg_cfg.user(user);
            }
            if let Some(password) = cfg.password.as_deref() {
                pg_cfg.password(password);
            }
            if let Some(host) = cfg.host.as_deref() {
                pg_cfg.host(host);
            }
            if let Some(port) = cfg.port {
                pg_cfg.port(port);
            }
            if let Some(dbname) = cfg.dbname.as_deref() {
                pg_cfg.dbname(dbname);
            }

            let pool = PgManager::new(pg_cfg).build_pool().await?;
            let typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;
            run_typed_pg_roundtrip(typed_conn).await?;
            println!("typed-postgres backend run successful");
        }

        // LibSQL (optional feature)
        #[cfg(feature = "libsql")]
        {
            let cap = ConfigAndPool::libsql_builder(":memory:".to_string())
                .build()
                .await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS custom_logic_txn (id INTEGER PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            println!("libsql backend run successful");
        }

        // Turso (optional feature)
        #[cfg(feature = "turso")]
        {
            let cap = ConfigAndPool::turso_builder(":memory:".to_string())
                .build()
                .await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS custom_logic_txn (id INTEGER PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            println!("turso backend run successful");
        }

        Ok::<(), SqlMiddlewareDbError>(())
    })?;

    Ok(())
}
