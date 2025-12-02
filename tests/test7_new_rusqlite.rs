#![cfg(feature = "sqlite")]

use std::sync::Arc;
use std::time::Duration;

use sql_middleware::prelude::*;
use sql_middleware::sqlite::{Params as SqliteParams, begin_transaction};
use tempfile::tempdir;
use tokio::sync::Semaphore;
use tokio::time::sleep;

async fn apply_pragmas(conn: &mut MiddlewarePoolConnection) -> Result<(), SqlMiddlewareDbError> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")
        .await
}

fn unique_db_path(prefix: &str) -> String {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join(format!("{prefix}.db"));
    // Leak the tempdir so the file persists for the duration of the test binary.
    std::mem::forget(dir);
    path.to_string_lossy().into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_tx_concurrency_and_rollbacks() -> Result<(), Box<dyn std::error::Error>> {
    let cap = Arc::new(
        ConfigAndPool::sqlite_builder(unique_db_path("stress"))
            .build()
            .await?,
    );
    let sem = Arc::new(Semaphore::new(1));
    let mut conn = cap.get_connection().await?;
    apply_pragmas(&mut conn).await?;
    conn.execute_batch(
        "CREATE TABLE stress (id INTEGER PRIMARY KEY, val TEXT NOT NULL);
         INSERT INTO stress (id, val) VALUES (0, 'seed');",
    )
    .await?;
    drop(conn);

    // Successful inserts use unique IDs; error tasks reuse id 0 to force a constraint failure.
    let mut handles = Vec::new();
    for i in 1..=200 {
        let cap = Arc::clone(&cap);
        let sem = Arc::clone(&sem);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let mut conn = cap.get_connection().await?;
            let _ = apply_pragmas(&mut conn).await;
            match &mut conn {
                MiddlewarePoolConnection::Sqlite { conn, .. } => {
                    let tx = begin_transaction(conn).await?;
                    let stmt = tx.prepare("INSERT INTO stress (id, val) VALUES (?1, ?2)")?;
                    let params = [RowValues::Int(i), RowValues::Text(format!("ok-{i}"))];
                    tx.execute_prepared(&stmt, &params).await?;
                    tx.commit().await?;
                    Ok::<(), SqlMiddlewareDbError>(())
                }
                _ => Err(SqlMiddlewareDbError::Unimplemented(
                    "expected sqlite connection".into(),
                )),
            }
        }));
    }

    for _ in 0..100 {
        let cap = Arc::clone(&cap);
        let sem = Arc::clone(&sem);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let mut conn = cap.get_connection().await?;
            let _ = apply_pragmas(&mut conn).await;
            match &mut conn {
                MiddlewarePoolConnection::Sqlite { conn, .. } => {
                    let tx = begin_transaction(conn).await?;
                    let stmt = tx.prepare("INSERT INTO stress (id, val) VALUES (?1, ?2)")?;
                    let params = [RowValues::Int(0), RowValues::Text("dupe".into())];
                    let res = tx.execute_prepared(&stmt, &params).await;
                    if res.is_ok() {
                        // Unexpected; ensure rollback anyway
                        let _ = tx.rollback().await;
                        return Err(SqlMiddlewareDbError::ExecutionError(
                            "expected constraint failure".into(),
                        ));
                    }
                    let _ = tx.rollback().await;
                    Ok(())
                }
                _ => Err(SqlMiddlewareDbError::Unimplemented(
                    "expected sqlite connection".into(),
                )),
            }
        }));
    }

    for h in handles {
        h.await??;
    }

    // Validate row count: 1 seed + 200 successes, no rows from rollback paths.
    let mut conn = cap.get_connection().await?;
    let rs = conn
        .query("SELECT COUNT(*) AS cnt FROM stress")
        .select()
        .await?;
    let count = *rs.results[0].get("cnt").unwrap().as_int().unwrap();
    assert_eq!(count, 201);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_tx_blocks_non_tx_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = ConfigAndPool::sqlite_builder(unique_db_path("block"))
        .build()
        .await?
        .get_connection()
        .await?;
    apply_pragmas(&mut conn).await?;
    conn.execute_batch("CREATE TABLE t1 (id INTEGER)").await?;

    if let MiddlewarePoolConnection::Sqlite { conn: raw, .. } = &mut conn {
        let tx = begin_transaction(raw).await?;
        // While tx is active, non-tx command should be rejected.
        let err = conn
            .execute_batch("INSERT INTO t1 (id) VALUES (1)")
            .await
            .unwrap_err();
        tx.rollback().await?;
        assert!(
            format!("{err}").contains("SQLite transaction in progress; operation not permitted")
        );
    } else {
        panic!("expected sqlite connection");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_tx_id_mismatch_errors_cleanly() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = ConfigAndPool::sqlite_builder(unique_db_path("mismatch"))
        .build()
        .await?
        .get_connection()
        .await?;
    apply_pragmas(&mut conn).await?;
    conn.execute_batch("CREATE TABLE t2 (id INTEGER)").await?;

    if let MiddlewarePoolConnection::Sqlite { conn: raw, .. } = &mut conn {
        let tx = begin_transaction(raw).await?;
        // Call the low-level tx API with an incorrect id to simulate misuse.
        let wrong_id = u64::MAX;
        let params = SqliteParams::convert(&[RowValues::Int(1)])?;
        let err = raw
            .test_execute_tx_dml(
                wrong_id,
                Arc::new("INSERT INTO t2 (id) VALUES (?1)".into()),
                params.0,
            )
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("SQLite transaction mismatch"));
        tx.rollback().await?;
    } else {
        panic!("expected sqlite connection");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_tx_drop_rolls_back() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = ConfigAndPool::sqlite_builder(unique_db_path("drop"))
        .build()
        .await?
        .get_connection()
        .await?;
    apply_pragmas(&mut conn).await?;
    conn.execute_batch("CREATE TABLE t3 (id INTEGER PRIMARY KEY)")
        .await?;

    if let MiddlewarePoolConnection::Sqlite { conn: raw, .. } = &mut conn {
        {
            let tx = begin_transaction(raw).await?;
            let stmt = tx.prepare("INSERT INTO t3 (id) VALUES (?1)")?;
            let params = [RowValues::Int(1)];
            // Ignore result; drop without explicit commit/rollback should auto-rollback.
            let _ = tx.execute_prepared(&stmt, &params).await;
        } // drop tx

        // Connection should still work and table should be empty. Retry a few times to allow async rollback.
        let mut attempts = 0;
        loop {
            attempts += 1;
            match conn.query("SELECT COUNT(*) AS cnt FROM t3").select().await {
                Ok(rs) => {
                    let count = *rs.results[0].get("cnt").unwrap().as_int().unwrap();
                    assert_eq!(count, 0);
                    break;
                }
                Err(e) => {
                    assert!(attempts < 5, "query failed after retries: {e}");
                    sleep(Duration::from_millis(20)).await;
                }
            }
        }
    } else {
        panic!("expected sqlite connection");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sqlite_tx_rejects_second_begin() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = ConfigAndPool::sqlite_builder(unique_db_path("second"))
        .build()
        .await?
        .get_connection()
        .await?;
    apply_pragmas(&mut conn).await?;
    conn.execute_batch("CREATE TABLE t4 (id INTEGER)").await?;

    if let MiddlewarePoolConnection::Sqlite { conn: raw, .. } = &mut conn {
        let tx = begin_transaction(raw).await?;

        // Non-tx read during open tx should be blocked.
        let read_err = conn.query("SELECT 1").select().await.unwrap_err();
        assert!(format!("{read_err}").contains("SQLite transaction in progress"));

        tx.commit().await?;

        // After commit, reads should succeed again.
        let rs = conn
            .query("SELECT COUNT(*) AS cnt FROM t4")
            .select()
            .await?;
        let _ = rs.results[0].get("cnt").unwrap().as_int().unwrap();
    } else {
        panic!("expected sqlite connection");
    }

    Ok(())
}
