use criterion::{criterion_group, criterion_main};
#[cfg(feature = "postgres")]
use sql_middleware::benchmark::postgres::{benchmark_postgres, cleanup_postgres};
use sql_middleware::benchmark::sqlite::{benchmark_sqlite, cleanup_sqlite};
use std::sync::LazyLock;
use tokio::runtime::Runtime;

#[allow(dead_code)]
struct DatabaseCleanup;

impl Drop for DatabaseCleanup {
    fn drop(&mut self) {
        #[cfg(feature = "postgres")]
        cleanup_postgres();
        cleanup_sqlite();
    }
}

static _CLEANUP: LazyLock<DatabaseCleanup> = LazyLock::new(|| DatabaseCleanup);

// Shared runtime for all benchmarks to avoid dual runtimes
static SHARED_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| Runtime::new().unwrap());

fn sqlite_benches_wrapper(c: &mut criterion::Criterion) {
    benchmark_sqlite(c, &SHARED_RUNTIME);
}

#[cfg(feature = "postgres")]
fn postgres_benches_wrapper(c: &mut criterion::Criterion) {
    benchmark_postgres(c, &SHARED_RUNTIME);
}

criterion_group!(sqlite_benches, sqlite_benches_wrapper);
#[cfg(feature = "postgres")]
criterion_group!(postgres_benches, postgres_benches_wrapper);
#[cfg(feature = "postgres")]
criterion_main!(sqlite_benches, postgres_benches);
#[cfg(not(feature = "postgres"))]
criterion_main!(sqlite_benches);
