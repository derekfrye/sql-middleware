#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

//! SQLx benchmark mirroring the middleware multi-thread pool checkout benchmark.
//! Focuses on async fan-out patterns (`spawn` + pooled connections) so we can
//! directly compare middleware overheads against idiomatic SQLx usage.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
    ConnectOptions, Executor, Row, SqlitePool,
};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

const SQLITE_SELECT: &str = "SELECT id, name, score, active FROM test WHERE id = ?1";

#[derive(Debug)]
#[allow(dead_code)]
struct BenchRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

impl BenchRow {
    fn from_sqlx(row: SqliteRow) -> Self {
        let active = row
            .try_get::<i64, _>(3)
            .map(|value| value != 0)
            .or_else(|_| row.try_get(3))
            .expect("extract active");
        Self {
            id: row.try_get(0).expect("extract id"),
            name: row.try_get(1).expect("extract name"),
            score: row.try_get(2).expect("extract score"),
            active,
        }
    }
}

struct Dataset {
    path: String,
    ids: Vec<i64>,
}

impl Dataset {
    fn path(&self) -> &str {
        &self.path
    }

    fn ids(&self) -> &[i64] {
        &self.ids
    }
}

fn shared_dataset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmark_sqlite_multithread_lookup.db")
}

static TOKIO_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("create tokio runtime"));

static BENCH_CONCURRENCY: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("BENCH_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8)
        .max(1)
});

static DATASET: LazyLock<Dataset> = LazyLock::new(|| {
    TOKIO_RUNTIME.block_on(async {
        let row_count = lookup_row_count_to_run();
        let path = shared_dataset_path();
        prepare_sqlite_dataset(path.as_ref(), row_count)
            .await
            .expect("prepare sqlite dataset");

        let mut ids: Vec<i64> = (1..=row_count as i64).collect();
        let mut rng = ChaCha8Rng::seed_from_u64(9_876_543_210);
        ids.shuffle(&mut rng);

        Dataset {
            path: path.to_string_lossy().into_owned(),
            ids,
        }
    })
});

// Pay the pool construction cost once up-front, keeping our benchmarks focused
// on connection checkout and query execution.
static SQLX_POOL: LazyLock<SqlitePool> = LazyLock::new(|| {
    let dataset = &*DATASET;
    let concurrency = *BENCH_CONCURRENCY as u32;
    TOKIO_RUNTIME.block_on(async {
        let options = SqliteConnectOptions::from_str(dataset.path())
            .expect("create connect options")
            .create_if_missing(true)
            .disable_statement_logging();
        SqlitePoolOptions::new()
            .max_connections(concurrency.max(1))
            .connect_with(options)
            .await
            .expect("create sqlx pool")
    })
});

fn lookup_row_count_to_run() -> usize {
    std::env::var("BENCH_LOOKUPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            std::env::var("BENCH_ROWS")
                .ok()
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(1024)
}

async fn prepare_sqlite_dataset(path: &Path, row_count: usize) -> Result<(), sqlx::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }

    let path_str = path.to_string_lossy();
    let options = SqliteConnectOptions::from_str(&path_str)?
        .create_if_missing(true)
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await?;

    sqlx::query(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE test (
            id      INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            score   REAL NOT NULL,
            active  INTEGER NOT NULL
        );
        ",
    )
    .execute(&pool)
    .await?;

    let mut tx = pool.begin().await?;
    for id in 1..=row_count as i64 {
        let name = format!("name-{id}");
        let score = id as f64 * 0.5;
        let active = (id % 2 == 0) as i64;
        sqlx::query("INSERT INTO test (id, name, score, active) VALUES (?1, ?2, ?3, ?4)")
            .bind(id)
            .bind(name)
            .bind(score)
            .bind(active)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    pool.close().await;

    Ok(())
}

fn chunk_size(total: usize, concurrency: usize) -> usize {
    if concurrency == 0 {
        return total.max(1);
    }
    (total + concurrency - 1) / concurrency
}

async fn sqlx_parallel_select(
    pool: &SqlitePool,
    ids: &[i64],
    concurrency: usize,
) -> Result<(), sqlx::Error> {
    let per_worker = chunk_size(ids.len(), concurrency);
    let mut join_set = JoinSet::new();

    for chunk in ids.chunks(per_worker).filter(|chunk| !chunk.is_empty()) {
        let chunk = chunk.to_vec();
        let pool = pool.clone();
        join_set.spawn(async move {
            let mut conn = pool.acquire().await?;
            for id in chunk {
                let row = sqlx::query(SQLITE_SELECT)
                    .bind(id)
                    .fetch_one(&mut *conn)
                    .await?;
                let data = BenchRow::from_sqlx(row);
                // try to prevent compiler optimizing away the work we're timing
                black_box(data);
            }
            Ok::<(), sqlx::Error>(())
        });
    }

    while let Some(outcome) = join_set.join_next().await {
        let result = outcome.expect("sqlx parallel worker panicked");
        result?;
    }

    Ok(())
}

async fn sqlx_parallel_checkout(pool: &SqlitePool, concurrency: usize) -> Result<(), sqlx::Error> {
    let mut join_set = JoinSet::new();
    for _ in 0..concurrency.max(1) {
        let pool = pool.clone();
        join_set.spawn(async move {
            let conn = pool.acquire().await?;
            drop(conn);
            Ok::<(), sqlx::Error>(())
        });
    }

    while let Some(outcome) = join_set.join_next().await {
        let result = outcome.expect("sqlx checkout worker panicked");
        result?;
    }

    Ok(())
}

async fn sqlx_parallel_prepare(pool: &SqlitePool, concurrency: usize) -> Result<(), sqlx::Error> {
    let mut join_set = JoinSet::new();
    for _ in 0..concurrency.max(1) {
        let pool = pool.clone();
        join_set.spawn(async move {
            let mut conn = pool.acquire().await?;
            let statement = conn.prepare(SQLITE_SELECT).await?;
            drop(statement);
            Ok::<(), sqlx::Error>(())
        });
    }

    while let Some(outcome) = join_set.join_next().await {
        let result = outcome.expect("sqlx prepare worker panicked");
        result?;
    }

    Ok(())
}

fn benchmark_sqlx_parallel_select(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;
    let pool = SQLX_POOL.clone();
    let concurrency = *BENCH_CONCURRENCY;

    group.bench_function(BenchmarkId::new("sqlx_parallel_select", concurrency), |b| {
        let ids = ids.clone();
        let pool = pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let pool = pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    sqlx_parallel_select(&pool, &ids, concurrency)
                        .await
                        .expect("sqlx parallel select");
                    total += start.elapsed();
                }
                total
            }
        });
    });
}

fn benchmark_sqlx_pool_checkout(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let pool = SQLX_POOL.clone();
    let concurrency = *BENCH_CONCURRENCY;

    group.bench_function(BenchmarkId::new("sqlx_pool_checkout", concurrency), |b| {
        let pool = pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    sqlx_parallel_checkout(&pool, concurrency)
                        .await
                        .expect("sqlx pool checkout");
                    total += start.elapsed();
                }
                total
            }
        });
    });
}

fn benchmark_sqlx_prepare_parallel(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let pool = SQLX_POOL.clone();
    let concurrency = *BENCH_CONCURRENCY;

    group.bench_function(BenchmarkId::new("sqlx_prepare_parallel", concurrency), |b| {
        let pool = pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    sqlx_parallel_prepare(&pool, concurrency)
                        .await
                        .expect("sqlx prepare parallel");
                    total += start.elapsed();
                }
                total
            }
        });
    });
}

fn sqlite_multithread_pool_checkout_sqlx(c: &mut Criterion) {
    let dataset = &*DATASET;
    let mut group = c.benchmark_group("sqlite_multithread_pool_checkout_sqlx");
    group.throughput(Throughput::Elements(dataset.ids().len() as u64));

    benchmark_sqlx_parallel_select(&mut group);
    benchmark_sqlx_pool_checkout(&mut group);
    benchmark_sqlx_prepare_parallel(&mut group);

    group.finish();
}

criterion_group!(benches, sqlite_multithread_pool_checkout_sqlx);
criterion_main!(benches);
