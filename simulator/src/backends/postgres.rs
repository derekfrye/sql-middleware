use async_trait::async_trait;
use sql_middleware::middleware::{
    ConfigAndPool, MiddlewarePoolConnection, PgConfig, PostgresOptions,
};

use crate::backends::{Backend, BackendError};
use crate::args::SimConfig;

pub(crate) struct PostgresBackend {
    pool: ConfigAndPool,
}

impl PostgresBackend {
    pub(crate) async fn new(config: &SimConfig) -> Result<Self, BackendError> {
        let config = pg_config_from_args(config)?;
        let pool = ConfigAndPool::new_postgres(PostgresOptions::new(config)).await?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl Backend for PostgresBackend {
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

fn pg_config_from_args(config: &SimConfig) -> Result<PgConfig, BackendError> {
    let mut cfg = PgConfig::new();
    cfg.dbname = Some(required_arg("pg-dbname", config.pg_dbname.clone())?);
    cfg.host = Some(required_arg("pg-host", config.pg_host.clone())?);
    cfg.port = Some(required_arg("pg-port", config.pg_port)?);
    cfg.user = Some(required_arg("pg-user", config.pg_user.clone())?);
    cfg.password = Some(required_arg("pg-password", config.pg_password.clone())?);
    Ok(cfg)
}

fn required_arg<T>(flag: &str, value: Option<T>) -> Result<T, BackendError> {
    value.ok_or_else(|| {
        BackendError::Init(format!(
            "missing required --{flag} for postgres backend"
        ))
    })
}
