use crate::pool::MiddlewarePoolConnection;

#[cfg(feature = "sqlite")]
use crate::sqlite::SqliteConnection;

/// Outcome returned by committing or rolling back a backend transaction.
///
/// Most backends do not need to surface anything after commit/rollback, but `SQLite`
/// consumes the pooled connection for the transaction and needs to hand it back
/// (with its translation flag) so callers can keep using the pooled wrapper.
///
/// If you started a raw `SQLite` transaction, continue using the connection from the returned
/// outcome instead of the pre-transaction wrapper to preserve the translation flag and pool state.
#[derive(Debug, Default)]
pub struct TxOutcome {
    restored_connection: Option<MiddlewarePoolConnection>,
}

impl TxOutcome {
    /// Outcome with no connection to restore (common for Postgres, `LibSQL`, Turso, MSSQL).
    #[must_use]
    pub fn without_restored_connection() -> Self {
        Self {
            restored_connection: None,
        }
    }

    /// Outcome that includes a connection restored to its pooled wrapper.
    #[must_use]
    pub fn with_restored_connection(conn: MiddlewarePoolConnection) -> Self {
        Self {
            restored_connection: Some(conn),
        }
    }

    /// Borrow the restored connection, if present.
    #[must_use]
    pub fn restored_connection(&self) -> Option<&MiddlewarePoolConnection> {
        self.restored_connection.as_ref()
    }

    /// Consume the outcome and take ownership of the restored connection, if present.
    pub fn into_restored_connection(self) -> Option<MiddlewarePoolConnection> {
        self.restored_connection
    }

    /// Restore any pooled connection contained in this outcome back into the caller's slot.
    ///
    /// This is primarily useful for `SQLite`, where the transaction API consumes the pooled wrapper
    /// and returns it on commit/rollback. Other backends return an empty outcome, making this a
    /// no-op.
    pub fn restore_into(self, conn_slot: &mut MiddlewarePoolConnection) {
        if let Some(conn) = self.into_restored_connection() {
            *conn_slot = conn;
        }
    }

    /// Consume the outcome and unwrap the `SQLite` connection + translation flag.
    #[cfg(feature = "sqlite")]
    pub fn into_sqlite_parts(self) -> Option<(SqliteConnection, bool)> {
        match self.restored_connection {
            Some(MiddlewarePoolConnection::Sqlite {
                mut conn,
                translate_placeholders,
            }) => conn.take().map(|conn| (conn, translate_placeholders)),
            _ => None,
        }
    }
}
