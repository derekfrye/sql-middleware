use std::{future::Future, marker::PhantomData, sync::atomic::AtomicBool};

use bb8::{ManageConnection, Pool, PooledConnection};
use tokio_postgres::{Client, NoTls};

use crate::middleware::SqlMiddlewareDbError;

/// Marker types for typestate
pub enum Idle {}
pub enum InTx {}

/// bb8 manager for Postgres clients.
pub struct PgManager {
    pub(crate) config: tokio_postgres::Config,
}

impl PgManager {
    #[must_use]
    pub fn new(config: tokio_postgres::Config) -> Self {
        Self { config }
    }

    /// Build a pool from this manager.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if pool creation fails.
    pub async fn build_pool(self) -> Result<Pool<PgManager>, SqlMiddlewareDbError> {
        Pool::builder()
            .build(self)
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("postgres pool error: {e}")))
    }
}

impl ManageConnection for PgManager {
    type Connection = Client;
    type Error = tokio_postgres::Error;

    #[allow(clippy::manual_async_fn)]
    fn connect(&self) -> impl Future<Output = Result<Self::Connection, Self::Error>> + Send {
        let cfg = self.config.clone();
        async move {
            let debug = std::env::var_os("SQL_MIDDLEWARE_PG_DEBUG").is_some();
            if debug {
                eprintln!(
                    "[sql-mw][pg] connect start hosts={:?} db={:?} user={:?}",
                    cfg.get_hosts(),
                    cfg.get_dbname(),
                    cfg.get_user()
                );
            }
            let (client, connection) = cfg.connect(NoTls).await?;
            if debug {
                eprintln!("[sql-mw][pg] connect established");
            }
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
    pub(crate) conn: Option<PooledConnection<'static, PgManager>>,
    /// True when a transaction is in-flight and needs rollback if dropped.
    pub(crate) needs_rollback: bool,
    pub(crate) _state: PhantomData<State>,
}

impl PgConnection<Idle> {
    /// Checkout a connection from the pool.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if acquiring the connection fails.
    pub async fn from_pool(pool: &Pool<PgManager>) -> Result<Self, SqlMiddlewareDbError> {
        let conn = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("postgres checkout error: {e}"))
        })?;
        Ok(Self::new(conn, false))
    }
}

impl<State> PgConnection<State> {
    pub(crate) fn new(conn: PooledConnection<'static, PgManager>, needs_rollback: bool) -> Self {
        Self {
            conn: Some(conn),
            needs_rollback,
            _state: PhantomData,
        }
    }

    pub(crate) fn conn_mut(&mut self) -> &mut PooledConnection<'static, PgManager> {
        self.conn
            .as_mut()
            .expect("postgres connection already taken")
    }

    pub(crate) fn take_conn(
        &mut self,
    ) -> Result<PooledConnection<'static, PgManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("postgres connection already taken".into())
        })
    }
}

pub(crate) static SKIP_DROP_ROLLBACK: AtomicBool = AtomicBool::new(false);
