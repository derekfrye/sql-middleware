use async_trait::async_trait;
use sql_middleware::middleware::{ConfigAndPool, MiddlewarePoolConnection, TursoOptions};

use crate::backends::{Backend, BackendError};
use crate::args::SimConfig;

pub(crate) struct TursoBackend {
    pool: ConfigAndPool,
}

impl TursoBackend {
    pub(crate) async fn new(config: &SimConfig) -> Result<Self, BackendError> {
        let pool = ConfigAndPool::new_turso(TursoOptions::new(config.turso_db_path.clone())).await?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl Backend for TursoBackend {
    async fn checkout(&self) -> Result<MiddlewarePoolConnection, BackendError> {
        Ok(self.pool.get_connection().await?)
    }

    async fn begin(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        conn.execute_batch("BEGIN").await?;
        Ok(())
    }

    async fn commit(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        conn.execute_batch("COMMIT").await?;
        Ok(())
    }

    async fn rollback(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        conn.execute_batch("ROLLBACK").await?;
        Ok(())
    }

    async fn execute(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        _in_tx: bool,
    ) -> Result<(), BackendError> {
        conn.execute_batch(sql).await?;
        Ok(())
    }

    async fn query(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        _in_tx: bool,
    ) -> Result<sql_middleware::ResultSet, BackendError> {
        Ok(conn.query(sql).select().await?)
    }

    async fn sleep(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}
