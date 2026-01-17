macro_rules! impl_typed_backend {
    ($conn:ident, $idle:ty, $intx:ty) => {
        impl Queryable for $conn<$idle> {
            fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
                self.query(sql)
            }
        }

        impl Queryable for $conn<$intx> {
            fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
                self.query(sql)
            }
        }

        impl TypedConnOps for $conn<$idle> {
            #[allow(clippy::manual_async_fn)]
            fn execute_batch(
                &mut self,
                sql: &str,
            ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>> {
                async move { self.execute_batch(sql).await }
            }

            #[allow(clippy::manual_async_fn)]
            fn dml(
                &mut self,
                query: &str,
                params: &[RowValues],
            ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>> {
                async move { self.dml(query, params).await }
            }

            #[allow(clippy::manual_async_fn)]
            fn select(
                &mut self,
                query: &str,
                params: &[RowValues],
            ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>> {
                async move { self.select(query, params).await }
            }
        }

        impl TypedConnOps for $conn<$intx> {
            #[allow(clippy::manual_async_fn)]
            fn execute_batch(
                &mut self,
                sql: &str,
            ) -> impl std::future::Future<Output = Result<(), SqlMiddlewareDbError>> {
                async move { self.execute_batch(sql).await }
            }

            #[allow(clippy::manual_async_fn)]
            fn dml(
                &mut self,
                query: &str,
                params: &[RowValues],
            ) -> impl std::future::Future<Output = Result<usize, SqlMiddlewareDbError>> {
                async move { self.dml(query, params).await }
            }

            #[allow(clippy::manual_async_fn)]
            fn select(
                &mut self,
                query: &str,
                params: &[RowValues],
            ) -> impl std::future::Future<Output = Result<ResultSet, SqlMiddlewareDbError>> {
                async move { self.select(query, params).await }
            }
        }

        impl BeginTx for $conn<$idle> {
            type Tx = $conn<$intx>;

            #[allow(clippy::manual_async_fn)]
            fn begin(
                self,
            ) -> impl std::future::Future<Output = Result<Self::Tx, SqlMiddlewareDbError>> {
                async move { self.begin().await }
            }
        }

        impl TxConn for $conn<$intx> {
            type Idle = $conn<$idle>;

            #[allow(clippy::manual_async_fn)]
            fn commit(
                self,
            ) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
                async move { self.commit().await }
            }

            #[allow(clippy::manual_async_fn)]
            fn rollback(
                self,
            ) -> impl std::future::Future<Output = Result<Self::Idle, SqlMiddlewareDbError>> {
                async move { self.rollback().await }
            }
        }
    };
}

pub(crate) use impl_typed_backend;
