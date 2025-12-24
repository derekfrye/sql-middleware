#![cfg(feature = "sqlite")]

use sql_middleware::middleware::ConfigAndPool;
use sql_middleware::pool::MiddlewarePoolConnection;
use sql_middleware::sqlite::transaction::{
    begin_transaction, set_rewrap_on_rollback_failure_for_tests,
};
use sql_middleware::SqlMiddlewareDbError;

/// When rollback fails with `SQLITE_BUSY`, we should evict instead of rewrapping the connection.
/// This test forces a busy error via a test hook and verifies both the new behavior (evict)
/// and the legacy behavior (rewrap) by toggling another test-only flag.
#[tokio::test(flavor = "current_thread")]
async fn sqlite_evicts_connection_when_rollback_busy() -> Result<(), SqlMiddlewareDbError> {
    let cap = ConfigAndPool::sqlite_builder("file::memory:?cache=shared".into())
        .build()
        .await?;

    // New behavior: failed rollback marks the worker broken and does not rewrap.
    let mut conn_slot = cap.get_connection().await?;
    set_force_rollback_busy(&mut conn_slot, true);
    set_rewrap_on_rollback_failure_for_tests(false);
    {
        let _tx = begin_transaction(&mut conn_slot).await?;
        // Drop with rollback busy -> should not rewrap.
    }
    if let MiddlewarePoolConnection::Sqlite { ref conn, .. } = conn_slot {
        assert!(
            conn.is_none(),
            "rollback failure should evict the connection instead of rewrapping it"
        );
    } else {
        panic!("expected sqlite connection");
    }

    // Legacy behavior: rewrap even when rollback fails.
    set_rewrap_on_rollback_failure_for_tests(true);
    let mut legacy_slot = cap.get_connection().await?;
    set_force_rollback_busy(&mut legacy_slot, true);
    {
        let _tx = begin_transaction(&mut legacy_slot).await?;
    }

    match legacy_slot {
        MiddlewarePoolConnection::Sqlite { ref conn, .. } => {
            if let Some(conn) = conn {
                conn.set_force_rollback_busy_for_tests(false);
            }
            assert!(
                conn.is_some(),
                "legacy path would have returned the connection to the pool"
            );
        }
        _ => panic!("expected sqlite connection"),
    }

    // Reset hooks to avoid impacting other tests.
    set_rewrap_on_rollback_failure_for_tests(false);

    Ok(())
}

fn set_force_rollback_busy(conn_slot: &mut MiddlewarePoolConnection, force: bool) {
    match conn_slot {
        MiddlewarePoolConnection::Sqlite { conn: Some(conn), .. } => {
            conn.set_force_rollback_busy_for_tests(force);
        }
        _ => panic!("expected sqlite connection"),
    }
}
