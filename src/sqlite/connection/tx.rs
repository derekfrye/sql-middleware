use crate::middleware::SqlMiddlewareDbError;

use super::{SqliteConnection, run_blocking};

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
        run_blocking(self.conn_handle(), move |guard| {
            guard
                .execute_batch("ROLLBACK")
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        self.in_transaction = false;
        Ok(())
    }
}
