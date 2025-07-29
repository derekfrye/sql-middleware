use criterion::{criterion_group, criterion_main};
use std::sync::LazyLock;
use sql_middleware::benchmark::{
    sqlite::{benchmark_sqlite, cleanup_sqlite},
    postgres::{benchmark_postgres, cleanup_postgres},
};
#[cfg(feature = "libsql")]
use sql_middleware::benchmark::libsql::{benchmark_libsql, cleanup_libsql};



#[allow(dead_code)]
struct DatabaseCleanup;

impl Drop for DatabaseCleanup {
    fn drop(&mut self) {
        cleanup_postgres();
        cleanup_sqlite();
        #[cfg(feature = "libsql")]
        cleanup_libsql();
    }
}

static _CLEANUP: LazyLock<DatabaseCleanup> = LazyLock::new(|| DatabaseCleanup);

criterion_group!(sqlite_benches, benchmark_sqlite);
criterion_group!(postgres_benches, benchmark_postgres);
#[cfg(feature = "libsql")]
criterion_group!(libsql_benches, benchmark_libsql);

// LibSQL benchmarks disabled due to instability/hanging issues
criterion_main!(sqlite_benches, postgres_benches);