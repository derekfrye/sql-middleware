use crate::query_builder::QueryBuilder;
use crate::typed::traits::Queryable;

use super::{AnyIdle, AnyTx};

impl Queryable for AnyIdle {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "postgres")]
            AnyIdle::Postgres(conn) => conn.query(sql),
            #[cfg(feature = "sqlite")]
            AnyIdle::Sqlite(conn) => conn.query(sql),
            #[cfg(feature = "turso")]
            AnyIdle::Turso(conn) => conn.query(sql),
            #[allow(unreachable_patterns)]
            _ => unreachable!("typed backends are not enabled"),
        }
    }
}

impl Queryable for AnyTx {
    fn query<'a>(&'a mut self, sql: &'a str) -> QueryBuilder<'a, 'a> {
        match self {
            #[cfg(feature = "postgres")]
            AnyTx::Postgres(conn) => conn.query(sql),
            #[cfg(feature = "sqlite")]
            AnyTx::Sqlite(conn) => conn.query(sql),
            #[cfg(feature = "turso")]
            AnyTx::Turso(conn) => conn.query(sql),
            #[allow(unreachable_patterns)]
            _ => unreachable!("typed backends are not enabled"),
        }
    }
}
