use std::fmt;
use std::sync::Arc;

use bb8::PooledConnection;
use tokio::task::spawn_blocking;

use crate::executor::QueryTarget;
use crate::middleware::{ResultSet, SqlMiddlewareDbError};
use crate::query_builder::QueryBuilder;
use crate::types::RowValues;

use super::config::{SharedSqliteConnection, SqliteManager, SqlitePooledConnection};
use super::params::Params;

/// Connection wrapper backed by a bb8 pooled SQLite connection.
pub struct SqliteConnection {
    pub(crate) conn: SqlitePooledConnection,
    in_transaction: bool,
}

impl SqliteConnection {
    pub(crate) fn new(conn: SqlitePooledConnection) -> Self {
        Self {
            conn,
            in_transaction: false,
        }
    }

    /// Execute a batch with implicit BEGIN/COMMIT.
    pub async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        self.ensure_not_in_tx("execute batch")?;
        let sql_owned = format!("BEGIN; {query}; COMMIT;");
        run_blocking(self.conn_handle(), move |guard| {
            guard
                .execute_batch(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
    }

    /// Execute a SELECT and materialize into a `ResultSet`.
    pub async fn execute_select<F>(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
        builder: F,
    ) -> Result<ResultSet, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Statement<'_>, &[rusqlite::types::Value]) -> Result<ResultSet, SqlMiddlewareDbError>
            + Send
            + 'static,
    {
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            builder(&mut stmt, &params_owned)
        })
        .await
    }

    /// Execute a DML statement and return rows affected.
    pub async fn execute_dml(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.ensure_not_in_tx("execute dml")?;
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            let refs: Vec<&dyn rusqlite::ToSql> = params_owned.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            let affected = stmt
                .execute(&refs[..])
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            usize::try_from(affected).map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!(
                    "sqlite affected rows conversion error: {e}"
                ))
            })
        })
        .await
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_sqlite(&mut self.conn, false), sql)
    }

    /// Begin a transaction, transitioning this connection into transactional mode.
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

    /// Execute a DML statement inside an open transaction.
    pub async fn execute_dml_in_tx(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
    ) -> Result<usize, SqlMiddlewareDbError> {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            let refs: Vec<&dyn rusqlite::ToSql> = params_owned.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            let affected = stmt
                .execute(&refs[..])
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            usize::try_from(affected).map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!(
                    "sqlite affected rows conversion error: {e}"
                ))
            })
        })
        .await
    }

    /// Execute a batch inside an open transaction without implicit commit.
    pub async fn execute_batch_in_tx(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let sql_owned = sql.to_owned();
        run_blocking(self.conn_handle(), move |guard| {
            guard
                .execute_batch(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await
    }

    /// Execute a query inside an open transaction and build a `ResultSet`.
    pub async fn execute_select_in_tx<F>(
        &mut self,
        query: &str,
        params: &[rusqlite::types::Value],
        builder: F,
    ) -> Result<ResultSet, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Statement<'_>, &[rusqlite::types::Value]) -> Result<ResultSet, SqlMiddlewareDbError>
            + Send
            + 'static,
    {
        if !self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction not active".into(),
            ));
        }
        let sql_owned = query.to_owned();
        let params_owned = params.to_vec();
        run_blocking(self.conn_handle(), move |guard| {
            let mut stmt = guard
                .prepare(&sql_owned)
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            builder(&mut stmt, &params_owned)
        })
        .await
    }

    /// Run synchronous `rusqlite` logic against the underlying pooled connection.
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

    /// Prepare a statement for repeated execution (auto-commit mode only).
    pub async fn prepare_statement(
        &mut self,
        query: &str,
    ) -> Result<crate::sqlite::prepared::SqlitePreparedStatement<'_>, SqlMiddlewareDbError> {
        if self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction in progress; operation not permitted (prepare statement)".into(),
            ));
        }
        let query_arc = Arc::new(query.to_owned());
        // warm the cache so repeated executions don't re-prepare.
        let query_clone = Arc::clone(&query_arc);
        run_blocking(self.conn_handle(), move |guard| {
            let _ = guard
                .prepare_cached(query_clone.as_ref())
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            Ok(())
        })
        .await?;
        Ok(crate::sqlite::prepared::SqlitePreparedStatement::new(self, query_arc))
    }

    pub(crate) fn into_target<'a>(&'a mut self, in_tx: bool) -> QueryTarget<'a> {
        QueryTarget::from_typed_sqlite(&mut self.conn, in_tx)
    }

    fn conn_handle(&self) -> SharedSqliteConnection {
        Arc::clone(self.conn.as_ref())
    }

    fn ensure_not_in_tx(&self, ctx: &str) -> Result<(), SqlMiddlewareDbError> {
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
            .field("in_transaction", &self.in_transaction)
            .finish()
    }
}

/// Adapter for query builder select (typed-sqlite target).
pub async fn select(
    conn: &mut PooledConnection<'static, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = Params::convert(params)?.0;
    let sql_owned = query.to_owned();
    let params_owned = converted.clone();
    let handle = Arc::clone(conn.as_ref());
    run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        super::query::build_result_set(&mut stmt, &params_owned)
    })
    .await
}

/// Adapter for query builder dml (typed-sqlite target).
pub async fn dml(
    conn: &mut PooledConnection<'static, SqliteManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = Params::convert(params)?.0;
    let sql_owned = query.to_owned();
    let params_owned = converted.clone();
    let handle = Arc::clone(conn.as_ref());
    run_blocking(handle, move |guard| {
        let mut stmt = guard
            .prepare(&sql_owned)
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params_owned.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let affected = stmt
            .execute(&refs[..])
            .map_err(SqlMiddlewareDbError::SqliteError)?;
        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "sqlite affected rows conversion error: {e}"
            ))
        })
    })
    .await
}

async fn run_blocking<F, R>(
    conn: SharedSqliteConnection,
    func: F,
) -> Result<R, SqlMiddlewareDbError>
where
    F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
    R: Send + 'static,
{
    spawn_blocking(move || {
        let mut guard = conn.blocking_lock();
        func(&mut guard)
    })
    .await
    .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!(
        "sqlite spawn_blocking join error: {e}"
    )))?
}

/// Apply WAL pragmas to a pooled connection.
pub async fn apply_wal_pragmas(conn: &mut SqlitePooledConnection) -> Result<(), SqlMiddlewareDbError> {
    let handle = Arc::clone(conn.as_ref());
    run_blocking(handle, |guard| {
        guard
            .execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(SqlMiddlewareDbError::SqliteError)
    })
    .await
}
