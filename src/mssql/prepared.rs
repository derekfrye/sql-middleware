use std::sync::Arc;

use tokio::sync::Mutex;

use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};

use super::{config::MssqlClient, query::build_result_set};

/// Prepared statement wrapper for SQL Server that holds onto a single connection.
///
/// This mirrors the non-transaction prepared handle exposed by other backends so
/// the public API stays consistent. **Tiberius does not expose a real prepared
/// statement type or caching**, so this wrapper simply stores the SQL text and
/// re-binds parameters on each execution against the same connection. It does
/// *not* amortize server-side compilation the way true prepared statements
/// would. Use this for API parity and connection pinning, not for preparation
/// cache wins.
#[derive(Clone)]
pub struct MssqlNonTxPreparedStatement {
    client: Arc<Mutex<MssqlClient>>,
    sql: Arc<String>,
}

impl std::fmt::Debug for MssqlNonTxPreparedStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MssqlNonTxPreparedStatement")
            .field("client", &"<MssqlClient>")
            .field("sql", &self.sql)
            .finish()
    }
}

impl MssqlNonTxPreparedStatement {
    /// Prepare a statement on the provided MSSQL client.
    ///
    /// The returned handle owns the client, so it should be created from a
    /// dedicated connection (e.g., via `create_mssql_client`).
    pub fn prepare(client: MssqlClient, sql: &str) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
            sql: Arc::new(sql.to_owned()),
        }
    }

    /// Execute the prepared statement as a query and materialize results.
    ///
    /// # Errors
    /// Returns an error if parameter conversion, execution, or result
    /// construction fails.
    pub async fn query(&self, params: &[RowValues]) -> Result<ResultSet, SqlMiddlewareDbError> {
        let mut client = self.client.lock().await;
        build_result_set(&mut client, &self.sql, params).await
    }

    /// Execute the prepared statement as DML and return affected rows.
    ///
    /// # Errors
    /// Returns an error if parameter conversion or execution fails.
    pub async fn execute(&self, params: &[RowValues]) -> Result<usize, SqlMiddlewareDbError> {
        let mut client = self.client.lock().await;
        let query_builder = super::query::bind_query_params(&self.sql, params);
        let exec_result = query_builder.execute(&mut *client).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("MSSQL prepared execute error: {e}"))
        })?;

        let rows_affected: u64 = exec_result.rows_affected().iter().sum();
        usize::try_from(rows_affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Invalid rows affected count: {e}"))
        })
    }

    /// Access the SQL text.
    #[must_use]
    pub fn sql(&self) -> &str {
        self.sql.as_str()
    }
}
