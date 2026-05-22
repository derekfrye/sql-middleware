use bb8::Pool;
use sql_middleware::middleware::{
    ConfigAndPool, DatabaseType, MiddlewarePool, MiddlewarePoolConnection,
};
use std::time::Duration;

use sql_middleware::RowValues;
use sql_middleware::sqlite::config::SqliteManager;
use sql_middleware::sqlite::params::Params;
use sql_middleware::sqlite::query::build_result_set;
use sql_middleware::sqlite::{SqliteConnection, apply_wal_pragmas};

use crate::backends::BackendError;

#[path = "sqlite/config.rs"]
mod config;
#[path = "sqlite/retry.rs"]
mod retry;
pub(crate) use config::SqliteBackendConfig;

pub(crate) struct SqliteBackend {
    pool: ConfigAndPool,
}

impl SqliteBackend {
    const BUSY_RETRIES: usize = 8;
    const INITIAL_RETRY_DELAY_MS: u64 = 5;
    const MAX_RETRY_DELAY_MS: u64 = 100;

    pub(crate) async fn new(config: SqliteBackendConfig) -> Result<Self, BackendError> {
        let pool_size = u32::try_from(config.pool_size.max(1))
            .map_err(|_| BackendError::Init("sqlite pool size does not fit in u32".to_string()))?;
        let manager = SqliteManager::new(config.db_path);
        let pool = Pool::builder()
            .max_size(pool_size)
            .build(manager)
            .await
            .map_err(|err| BackendError::Init(format!("sqlite pool error: {err}")))?;

        {
            let mut conn = pool
                .get_owned()
                .await
                .map_err(|err| BackendError::Init(format!("sqlite pool checkout error: {err}")))?;
            apply_wal_pragmas(&mut conn).await?;
        }

        Ok(Self {
            pool: ConfigAndPool {
                pool: MiddlewarePool::Sqlite(pool),
                db_type: DatabaseType::Sqlite,
                translate_placeholders: false,
                statement_cache_mode: sql_middleware::StatementCacheMode::Cached,
            },
        })
    }

    pub(crate) async fn checkout(&self) -> Result<MiddlewarePoolConnection, BackendError> {
        Ok(self.pool.get_connection().await?)
    }

    fn sqlite_conn_mut(
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<&mut SqliteConnection, BackendError> {
        match conn {
            MiddlewarePoolConnection::Sqlite { conn, .. } => conn.as_mut().ok_or_else(|| {
                BackendError::Init("SQLite connection already taken from pool wrapper".to_string())
            }),
            _ => Err(BackendError::Init(
                "SQLite backend called with non-sqlite connection".to_string(),
            )),
        }
    }

    pub(crate) async fn begin(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        let mut delay_ms = Self::INITIAL_RETRY_DELAY_MS;
        for attempt in 0..=Self::BUSY_RETRIES {
            let sqlite_conn = Self::sqlite_conn_mut(conn)?;
            match sqlite_conn.begin().await.map_err(BackendError::from) {
                Ok(result) => return Ok(result),
                Err(err) if retry::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    Self::sleep_before_retry(&mut delay_ms).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn commit(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        let mut delay_ms = Self::INITIAL_RETRY_DELAY_MS;
        for attempt in 0..=Self::BUSY_RETRIES {
            let sqlite_conn = Self::sqlite_conn_mut(conn)?;
            match sqlite_conn.commit().await.map_err(BackendError::from) {
                Ok(result) => return Ok(result),
                Err(err) if retry::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    Self::sleep_before_retry(&mut delay_ms).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn rollback(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        let mut delay_ms = Self::INITIAL_RETRY_DELAY_MS;
        for attempt in 0..=Self::BUSY_RETRIES {
            let sqlite_conn = Self::sqlite_conn_mut(conn)?;
            match sqlite_conn.rollback().await.map_err(BackendError::from) {
                Ok(result) => return Ok(result),
                Err(err) if retry::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    Self::sleep_before_retry(&mut delay_ms).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn execute(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<(), BackendError> {
        let mut delay_ms = Self::INITIAL_RETRY_DELAY_MS;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = if in_tx {
                Self::execute_in_tx(conn, sql).await
            } else {
                conn.execute_batch(sql).await.map_err(BackendError::from)
            };
            match result {
                Ok(result) => return Ok(result),
                Err(err) if retry::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    Self::sleep_before_retry(&mut delay_ms).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn query(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<sql_middleware::ResultSet, BackendError> {
        let mut delay_ms = Self::INITIAL_RETRY_DELAY_MS;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = if in_tx {
                Self::query_in_tx(conn, sql).await
            } else {
                conn.query(sql).select().await.map_err(BackendError::from)
            };
            match result {
                Ok(result) => return Ok(result),
                Err(err) if retry::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    Self::sleep_before_retry(&mut delay_ms).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn sleep(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    async fn execute_in_tx(
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
    ) -> Result<(), BackendError> {
        let sqlite_conn = Self::sqlite_conn_mut(conn)?;
        sqlite_conn
            .execute_batch_in_tx(sql)
            .await
            .map_err(BackendError::from)
    }

    async fn query_in_tx(
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
    ) -> Result<sql_middleware::ResultSet, BackendError> {
        let sqlite_conn = Self::sqlite_conn_mut(conn)?;
        let params: &[RowValues] = &[];
        let params = Params::convert(params).map_err(BackendError::from)?;
        sqlite_conn
            .execute_select_in_tx(sql, params.as_values(), build_result_set)
            .await
            .map_err(BackendError::from)
    }

    async fn sleep_before_retry(delay_ms: &mut u64) {
        tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
        *delay_ms = (*delay_ms * 2).min(Self::MAX_RETRY_DELAY_MS);
    }
}
