use criterion::{criterion_group, criterion_main};
use std::sync::LazyLock;
use sql_middleware::benchmark::{
    sqlite::{benchmark_sqlite, cleanup_sqlite},
    postgres::{benchmark_postgres, cleanup_postgres},
};



#[allow(dead_code)]
struct DatabaseCleanup;

impl Drop for DatabaseCleanup {
    fn drop(&mut self) {
        cleanup_postgres();
        cleanup_sqlite();
    }
}

static _CLEANUP: LazyLock<DatabaseCleanup> = LazyLock::new(|| DatabaseCleanup);

criterion_group!(sqlite_benches, benchmark_sqlite);
criterion_group!(postgres_benches, benchmark_postgres);
criterion_main!(sqlite_benches, postgres_benches);