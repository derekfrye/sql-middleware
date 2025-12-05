//! Experimental bb8-backed Postgres typestate API.
//! Provides `PgConnection<Idle>` / `PgConnection<InTx>` using an owned client
//! and explicit BEGIN/COMMIT/ROLLBACK.

use std::{future::Future, marker::PhantomData};

use bb8::{ManageConnection, Pool, PooledConnection};
use tokio_postgres::{Client, NoTls};

use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::postgres::params::Params as PgParams;
use crate::postgres::query::postgres_extract_value;
use crate::results::ResultSet;
use crate::query_builder::QueryBuilder;
use crate::executor::QueryTarget;
use crate::types::ParamConverter;

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// bb8 manager for Postgres clients.
pub struct PgManager {
    config: tokio_postgres::Config,
}

impl PgManager {
    #[must_use]
    pub fn new(config: tokio_postgres::Config) -> Self {
        Self { config }
    }
}

impl ManageConnection for PgManager {
    type Connection = Client;
    type Error = tokio_postgres::Error;

    #[allow(clippy::manual_async_fn)]
    fn connect(&self) -> impl Future<Output = Result<Self::Connection, Self::Error>> + Send {
        let cfg = self.config.clone();
        async move {
            let (client, connection) = cfg.connect(NoTls).await?;
            tokio::spawn(async move {
                if let Err(_e) = connection.await {
                    // drop error
                }
            });
            Ok(client)
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn is_valid(
        &self,
        conn: &mut Self::Connection,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move { conn.simple_query("SELECT 1").await.map(|_| ()) }
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}

/// Typestate wrapper around a pooled Postgres client.
pub struct PgConnection<State> {
    conn: PooledConnection<'static, PgManager>,
    _state: PhantomData<State>,
}

impl PgManager {
    /// Build a pool from this manager.
    pub async fn build_pool(self) -> Result<Pool<PgManager>, SqlMiddlewareDbError> {
        Pool::builder()
            .build(self)
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("postgres pool error: {e}")))
    }
}

impl PgConnection<Idle> {
    /// Checkout a connection from the pool.
    pub async fn from_pool(
        pool: &Pool<PgManager>,
    ) -> Result<Self, SqlMiddlewareDbError> {
        let conn = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("postgres checkout error: {e}"))
        })?;
        Ok(Self {
            conn,
            _state: PhantomData,
        })
    }

    /// Begin an explicit transaction.
    pub async fn begin(self) -> Result<PgConnection<InTx>, SqlMiddlewareDbError> {
        self.conn
            .simple_query("BEGIN")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres begin error: {e}")))?;
        Ok(PgConnection {
            conn: self.conn,
            _state: PhantomData,
        })
    }

    /// Auto-commit batch (BEGIN/COMMIT around it).
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        let script = format!("BEGIN; {sql}; COMMIT;");
        self.conn
            .batch_execute(&script)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres batch error: {e}")))
    }

    /// Auto-commit DML.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = PgParams::convert_sql_params(params, crate::types::ConversionMode::Execute)?;
        let rows = self
            .conn
            .execute(query, converted.as_refs())
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres execute error: {e}")))?;
        usize::try_from(rows).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "postgres affected rows conversion error: {e}"
            ))
        })
    }

    /// Auto-commit SELECT.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = PgParams::convert_sql_params(params, crate::types::ConversionMode::Query)?;
        let rows = self
            .conn
            .query(query, converted.as_refs())
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres query error: {e}")))?;
        build_result_set_from_rows(&rows)
    }

    /// Start a query builder (auto-commit per operation).
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_postgres(&mut self.conn, false), sql)
    }
}

impl PgConnection<InTx> {
    /// Commit and return to idle.
    pub async fn commit(self) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        self.conn
            .simple_query("COMMIT")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres commit error: {e}")))?;
        Ok(PgConnection {
            conn: self.conn,
            _state: PhantomData,
        })
    }

    /// Rollback and return to idle.
    pub async fn rollback(self) -> Result<PgConnection<Idle>, SqlMiddlewareDbError> {
        self.conn
            .simple_query("ROLLBACK")
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres rollback error: {e}")))?;
        Ok(PgConnection {
            conn: self.conn,
            _state: PhantomData,
        })
    }

    /// Execute batch inside the open transaction.
    pub async fn execute_batch(&mut self, sql: &str) -> Result<(), SqlMiddlewareDbError> {
        self.conn
            .batch_execute(sql)
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres tx batch error: {e}")))
    }

    /// Execute DML inside the open transaction.
    pub async fn dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = PgParams::convert_sql_params(params, crate::types::ConversionMode::Execute)?;
        let rows = self
            .conn
            .execute(query, converted.as_refs())
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres tx execute error: {e}")))?;
        usize::try_from(rows).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "postgres affected rows conversion error: {e}"
            ))
        })
    }

    /// Execute SELECT inside the open transaction.
    pub async fn select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = PgParams::convert_sql_params(params, crate::types::ConversionMode::Query)?;
        let rows = self
            .conn
            .query(query, converted.as_refs())
            .await
            .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres tx query error: {e}")))?;
        build_result_set_from_rows(&rows)
    }

    /// Start a query builder within the open transaction.
    pub fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new_target(QueryTarget::from_typed_postgres(&mut self.conn, true), sql)
    }
}

/// Adapter for query builder select (typed-postgres target).
pub async fn select(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let converted = PgParams::convert_sql_params(params, crate::types::ConversionMode::Query)?;
    let rows = conn
        .query(query, converted.as_refs())
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres select error: {e}")))?;
    build_result_set_from_rows(&rows)
}

/// Adapter for query builder dml (typed-postgres target).
pub async fn dml(
    conn: &mut PooledConnection<'_, PgManager>,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let converted = PgParams::convert_sql_params(params, crate::types::ConversionMode::Execute)?;
    let rows = conn
        .execute(query, converted.as_refs())
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("postgres dml error: {e}")))?;
    usize::try_from(rows).map_err(|e| {
        SqlMiddlewareDbError::ExecutionError(format!(
            "postgres affected rows conversion error: {e}"
        ))
    })
}

fn build_result_set_from_rows(rows: &[tokio_postgres::Row]) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut result_set = ResultSet::with_capacity(rows.len());
    if let Some(row) = rows.first() {
        let cols: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
        result_set.set_column_names(std::sync::Arc::new(cols));
    }

    for row in rows {
        let col_count = row.columns().len();
        let mut row_values = Vec::with_capacity(col_count);
        for idx in 0..col_count {
            row_values.push(postgres_extract_value(row, idx)?);
        }
        result_set.add_row_values(row_values);
    }

    Ok(result_set)
}
