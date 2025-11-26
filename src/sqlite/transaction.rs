use std::sync::Arc;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::middleware::{
    ConversionMode, ParamConverter, ResultSet, RowValues, SqlMiddlewareDbError,
};

use super::params::Params;
use super::worker::SqliteConnection;

/// Transaction handle for SQLite backed by the worker thread.
pub struct Tx {
    conn: SqliteConnection,
    tx_id: u64,
    completed: AtomicBool,
}

/// Prepared statement tied to a SQLite transaction.
pub struct Prepared {
    sql: Arc<String>,
}

/// Begin a transaction on the worker-owned SQLite connection.
///
/// # Errors
/// Returns an error if the worker fails to start a transaction.
pub async fn begin_transaction(conn: &SqliteConnection) -> Result<Tx, SqlMiddlewareDbError> {
    let tx_id = conn.begin_transaction().await?;
    Ok(Tx {
        conn: conn.clone(),
        tx_id,
        completed: AtomicBool::new(false),
    })
}

impl Tx {
    /// Test hook to fetch the underlying transaction id (not part of the public API surface).
    #[doc(hidden)]
    pub fn test_id(&self) -> u64 {
        self.tx_id
    }

    /// Prepare a statement within this transaction.
    ///
    /// # Errors
    /// Returns an error if the SQL cannot be queued for preparation.
    pub fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        Ok(Prepared {
            sql: Arc::new(sql.to_owned()),
        })
    }

    /// Execute a prepared statement as DML within this transaction.
    ///
    /// # Errors
    /// Returns an error if parameter conversion or execution fails.
    pub async fn execute_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Execute)?;

        self.conn
            .execute_tx_dml(self.tx_id, Arc::clone(&prepared.sql), converted.0)
            .await
    }

    /// Execute a prepared statement as a query within this transaction.
    ///
    /// # Errors
    /// Returns an error if parameter conversion or execution fails.
    pub async fn query_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted =
            <Params as ParamConverter>::convert_sql_params(params, ConversionMode::Query)?;

        self.conn
            .execute_tx_query(self.tx_id, Arc::clone(&prepared.sql), converted.0)
            .await
    }

    /// Execute a batch of SQL statements inside this transaction.
    ///
    /// # Errors
    /// Returns an error if execution fails.
    pub async fn execute_batch(&self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn.execute_tx_batch(self.tx_id, sql.to_owned()).await
    }

    /// Commit the transaction.
    ///
    /// # Errors
    /// Returns an error if the commit fails.
    pub async fn commit(&self) -> Result<(), SqlMiddlewareDbError> {
        self.conn.commit_tx(self.tx_id).await?;
        self.completed.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Roll back the transaction.
    ///
    /// # Errors
    /// Returns an error if the rollback fails.
    pub async fn rollback(&self) -> Result<(), SqlMiddlewareDbError> {
        self.conn.rollback_tx(self.tx_id).await?;
        self.completed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for Tx {
    fn drop(&mut self) {
        if self.completed.load(Ordering::SeqCst) {
            return;
        }
        // Best-effort async rollback to avoid leaving the worker in a stuck transaction loop.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let conn = self.conn.clone();
            let tx_id = self.tx_id;
            handle.spawn(async move {
                let _ = conn.rollback_tx(tx_id).await;
            });
        }
    }
}
