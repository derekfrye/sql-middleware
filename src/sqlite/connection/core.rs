use std::fmt;
use std::sync::Arc;

use crate::middleware::SqlMiddlewareDbError;

use crate::sqlite::config::{SharedSqliteConnection, SqlitePooledConnection};
use tokio::sync::oneshot;

/// Connection wrapper backed by a bb8 pooled `SQLite` connection.
pub struct SqliteConnection {
    pub(crate) conn: SqlitePooledConnection,
    pub(crate) in_transaction: bool,
}

impl SqliteConnection {
    pub(crate) fn new(conn: SqlitePooledConnection) -> Self {
        Self {
            conn,
            in_transaction: false,
        }
    }

    /// Run `func` on the pooled rusqlite connection while no other transaction is in flight.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ExecutionError` if the connection is in a transaction or the closure returns an error.
    pub async fn with_connection<F, R>(&self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        if self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction in progress; operation not permitted (with connection)".into(),
            ));
        }
        run_blocking(self.conn_handle(), func).await
    }

    pub(crate) fn conn_handle(&self) -> SharedSqliteConnection {
        Arc::clone(&*self.conn)
    }

    pub(crate) fn ensure_not_in_tx(&self, ctx: &str) -> Result<(), SqlMiddlewareDbError> {
        if self.in_transaction {
            Err(SqlMiddlewareDbError::ExecutionError(format!(
                "SQLite transaction in progress; operation not permitted ({ctx})"
            )))
        } else {
            Ok(())
        }
    }
}

impl fmt::Debug for SqliteConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteConnection")
            .field("conn", &self.conn)
            .field("in_transaction", &self.in_transaction)
            .finish()
    }
}

pub(crate) async fn run_blocking<F, R>(
    conn: SharedSqliteConnection,
    func: F,
) -> Result<R, SqlMiddlewareDbError>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    conn.execute(move |conn| {
        let _ = tx.send(func(conn));
    })?;
    rx.await.map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!("sqlite worker receive error: {e}"))
    })?
}

/// Apply WAL pragmas to a pooled connection.
///
/// # Errors
/// Returns `SqlMiddlewareDbError` if the PRAGMA statements cannot be executed.
pub async fn apply_wal_pragmas(
    conn: &mut SqlitePooledConnection,
) -> Result<(), SqlMiddlewareDbError> {
    let handle = Arc::clone(&*conn);
    run_blocking(handle, |guard| {
        guard
            .execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(SqlMiddlewareDbError::SqliteError)
    })
    .await
}
