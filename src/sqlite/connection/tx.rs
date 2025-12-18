use crate::middleware::SqlMiddlewareDbError;

use super::{SqliteConnection, run_blocking};
use crate::sqlite::config::SharedSqliteConnection;
use std::thread;
use std::time::Duration;

const ROLLBACK_BUSY_RETRIES: &[Duration] =
    &[Duration::from_millis(10), Duration::from_millis(25), Duration::from_millis(50)];

pub(crate) fn rollback_with_busy_retries(
    handle: SharedSqliteConnection,
) -> Result<(), SqlMiddlewareDbError> {
    if handle.force_rollback_busy_for_tests() {
        return Err(SqlMiddlewareDbError::SqliteError(
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::DatabaseBusy,
                    extended_code: rusqlite::ErrorCode::DatabaseBusy as i32,
                },
                None,
            ),
        ));
    }

    for (idx, delay) in ROLLBACK_BUSY_RETRIES.iter().copied().enumerate() {
        let result = handle.execute_blocking(|guard| {
            guard
                .execute_batch("ROLLBACK")
                .map_err(SqlMiddlewareDbError::SqliteError)
        });

        match &result {
            Ok(()) => return result,
            Err(SqlMiddlewareDbError::SqliteError(rusqlite::Error::SqliteFailure(err, _)))
                if err.code == rusqlite::ErrorCode::DatabaseBusy
                    && idx + 1 < ROLLBACK_BUSY_RETRIES.len() =>
            {
                thread::sleep(delay);
                continue;
            }
            _ => return result,
        }
    }

    Err(SqlMiddlewareDbError::ExecutionError(
        "rollback retries exhausted".into(),
    ))
}

impl SqliteConnection {
    /// Begin a transaction, transitioning this connection into transactional mode.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the transaction cannot be started or is already active.
    pub async fn begin(&mut self) -> Result<(), SqlMiddlewareDbError> {
        if self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction already in progress".into(),
            ));
        }
        run_blocking(self.conn_handle(), move |guard| {
            guard
                .execute_batch("BEGIN")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        self.in_transaction = true;
        Ok(())
    }

    /// Commit an open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if committing fails or no transaction is active.
    pub async fn commit(&mut self) -> Result<(), SqlMiddlewareDbError> {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        run_blocking(self.conn_handle(), move |guard| {
            guard
                .execute_batch("COMMIT")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        self.in_transaction = false;
        Ok(())
    }

    /// Roll back an open transaction.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if rolling back fails or no transaction is active.
    pub async fn rollback(&mut self) -> Result<(), SqlMiddlewareDbError> {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let result = rollback_with_busy_retries(self.conn_handle());
        if result.is_err() {
            self.mark_broken();
            return result;
        }
        self.in_transaction = false;
        result
    }
}
