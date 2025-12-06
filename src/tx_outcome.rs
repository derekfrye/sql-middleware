use crate::pool::MiddlewarePoolConnection;

#[cfg(feature = "sqlite")]
use crate::sqlite::SqliteConnection;

/// Outcome returned by committing or rolling back a backend transaction.
///
/// Most backends do not need to surface anything after commit/rollback, but `SQLite`
/// consumes the pooled connection for the transaction and needs to hand it back
/// (with its translation flag) so callers can keep using the pooled wrapper.
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
