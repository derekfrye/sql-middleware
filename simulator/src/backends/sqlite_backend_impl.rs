use async_trait::async_trait;
use sql_middleware::middleware::MiddlewarePoolConnection;

use crate::backends::{Backend, BackendError};

use super::sqlite::SqliteBackend;

#[async_trait]
impl Backend for SqliteBackend {
    async fn checkout(&self) -> Result<MiddlewarePoolConnection, BackendError> {
        self.checkout().await
    }

    async fn begin(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        self.begin(conn).await
    }

    async fn commit(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        self.commit(conn).await
    }

    async fn rollback(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        self.rollback(conn).await
    }

    async fn execute(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<(), BackendError> {
        self.execute(conn, sql, in_tx).await
    }

    async fn query(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<sql_middleware::ResultSet, BackendError> {
        self.query(conn, sql, in_tx).await
    }

    async fn sleep(&self, ms: u64) {
        self.sleep(ms).await;
    }
}
