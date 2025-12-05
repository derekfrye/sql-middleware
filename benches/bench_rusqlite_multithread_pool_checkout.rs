#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

//! Criterion benchmark comparing concurrent checkout/query patterns between
//! the sql-middleware abstraction and a direct `rusqlite` approach backed by a
//! simple connection pool. Each micro-benchmark fans out a batch of single-row
//! lookups across multiple workers to highlight middleware overheads that show
//! up in real-world async applications.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rusqlite::{Connection, Row, Result as RusqliteResult};
use sql_middleware::{ConfigAndPool, RowValues, SqlMiddlewareDbError};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

const SQLITE_SELECT: &str = "SELECT id, name, score, active FROM test WHERE id = ?1";

/// Holds the reusable on-disk `SQLite` dataset plus deterministic lookup IDs.
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

/// Row representation shared across benchmark variants to keep decode cost consistent.
#[derive(Debug)]
#[allow(dead_code)]
struct BenchRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

impl BenchRow {
    fn from_rusqlite(
        row: &Row<'_>,
    ) -> RusqliteResult<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            score: row.get(2)?,
            active: row
                .get::<_, i64>(3)
                .map(|value| value != 0)
                .or_else(|_| row.get(3))?,
        })
    }
}

/// Very small, blocking connection pool for direct `rusqlite` comparisons.
/// We don't use `deadpool` here because it is async and we want to
/// keep the baseline as close to most common `rusqlite` usage patterns.
#[derive(Clone)]
struct BlockingRusqlitePool {
    connections: Arc<Mutex<Vec<Connection>>>,
}

impl BlockingRusqlitePool {
    fn new(path: &str, size: usize) -> Self {
        let size = size.max(1);
        let mut connections = Vec::with_capacity(size);
        for _ in 0..size {
            let conn =
                Connection::open(path).expect("open rusqlite connection for benchmark");
            connections.push(conn);
        }
        Self {
            connections: Arc::new(Mutex::new(connections)),
        }
    }

    fn checkout(&self) -> BlockingConnectionGuard<'_> {
        BlockingConnectionGuard {
            pool: &self.connections,
            conn: Some(
                self.connections
                    .lock()
                    .expect("acquire rusqlite pool lock")
                    .pop()
                    .expect("rusqlite pool exhausted; concurrency > pool size"),
            ),
        }
    }
}

struct BlockingConnectionGuard<'a> {
    pool: &'a Mutex<Vec<Connection>>,
    conn: Option<Connection>,
}

impl BlockingConnectionGuard<'_> {
    fn connection(&mut self) -> &mut Connection {
        self.conn
            .as_mut()
            .expect("guard released connection unexpectedly")
    }
}

impl Drop for BlockingConnectionGuard<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool
                .lock()
                .expect("acquire rusqlite pool lock for drop")
                .push(conn);
        }
    }
}

// Shared runtime so both middleware and rusqlite baselines can spawn async work.
static TOKIO_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("create tokio runtime"));

// Dataset prepared once and reused across benchmark runs.
static DATASET: LazyLock<Dataset> = LazyLock::new(|| {
    let row_count = lookup_row_count_to_run();
    let path = PathBuf::from("benchmark_sqlite_multithread_lookup.db");
    prepare_sqlite_dataset(&path, row_count).expect("prepare sqlite dataset");

    let mut ids: Vec<i64> = (1..=row_count as i64).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(9_876_543_210);
    ids.shuffle(&mut rng);

    Dataset {
        path: path.to_string_lossy().into_owned(),
        ids,
    }
});

// Middleware pool initialised once so subsequent benchmark iterations exercise steady-state behaviour.
static MIDDLEWARE_CONFIG: LazyLock<ConfigAndPool> = LazyLock::new(|| {
    TOKIO_RUNTIME
        .block_on(ConfigAndPool::sqlite_builder(DATASET.path().to_string()).build())
        .expect("create middleware config and pool")
});

// Number of concurrent workers to launch for multi-threaded fan-out.
static BENCH_CONCURRENCY: LazyLock<usize> = LazyLock::new(|| concurrency_to_run().max(1));

/// Resolve how many lookups each iteration should perform.
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

/// Resolve how many worker tasks to run in parallel.
fn concurrency_to_run() -> usize {
    std::env::var("BENCH_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8)
}

/// Create a fresh `SQLite` file with predictable contents for repeatable runs.
fn prepare_sqlite_dataset(path: &Path, row_count: usize) -> RusqliteResult<()> {
    if path.exists() {
        let _ = fs::remove_file(path);
    }

    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE test (
            id      INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            score   REAL NOT NULL,
            active  INTEGER NOT NULL
        );
        ",
    )?;

    let tx = conn.transaction()?;
    {
        let mut insert_stmt =
            tx.prepare("INSERT INTO test (id, name, score, active) VALUES (?1, ?2, ?3, ?4)")?;

        for id in 1..=row_count as i64 {
            let name = format!("name-{id}");
            let score = id as f64 * 0.5;
            let active = id % 2 == 0;
            insert_stmt.execute(rusqlite::params![id, name, score, i32::from(active)])?;
        }
    }
    tx.commit()?;

    Ok(())
}

fn chunk_size(total: usize, concurrency: usize) -> usize {
    if concurrency == 0 {
        return total.max(1);
    }
    total.div_ceil(concurrency)
}

async fn middleware_parallel_select(
    config_and_pool: &ConfigAndPool,
    ids: &[i64],
    concurrency: usize,
) -> Result<(), SqlMiddlewareDbError> {
    let per_worker = chunk_size(ids.len(), concurrency);
    let mut join_set = JoinSet::new();

    for chunk in ids.chunks(per_worker).filter(|chunk| !chunk.is_empty()) {
        let config_and_pool = config_and_pool.clone();
        let chunk = chunk.to_vec();
        join_set.spawn(async move {
            let mut conn = config_and_pool.get_connection().await?;
            let mut prepared = conn.prepare_sqlite_statement(SQLITE_SELECT).await?;
            let mut params = vec![RowValues::Int(0)];
            for id in chunk {
                params[0] = RowValues::Int(id);
                let result = prepared.query(&params).await?;
                let row = result.results.first().ok_or_else(|| {
                    SqlMiddlewareDbError::ExecutionError(
                        "expected row from middleware query".to_string(),
                    )
                })?;
                black_box(row);
            }
            Ok::<(), SqlMiddlewareDbError>(())
        });
    }

    while let Some(outcome) = join_set.join_next().await {
        let result = outcome.expect("middleware worker panicked");
        result?;
    }

    Ok(())
}

async fn middleware_parallel_checkout(
    config_and_pool: &ConfigAndPool,
    concurrency: usize,
) -> Result<(), SqlMiddlewareDbError> {
    let mut join_set = JoinSet::new();
    for _ in 0..concurrency.max(1) {
        let config_and_pool = config_and_pool.clone();
        join_set.spawn(async move {
            let conn = config_and_pool.get_connection().await?;
            drop(conn);
            Ok::<(), SqlMiddlewareDbError>(())
        });
    }

    while let Some(outcome) = join_set.join_next().await {
        let result = outcome.expect("checkout worker panicked");
        result?;
    }

    Ok(())
}

async fn rusqlite_parallel_select(
    pool: BlockingRusqlitePool,
    ids: &[i64],
    concurrency: usize,
) -> RusqliteResult<()> {
    let per_worker = chunk_size(ids.len(), concurrency);
    let mut handles = Vec::new();

    for chunk in ids.chunks(per_worker).filter(|chunk| !chunk.is_empty()) {
        let chunk = chunk.to_vec();
        let pool = pool.clone();
        handles.push(tokio::task::spawn_blocking(move || {
            let mut guard = pool.checkout();
            let conn = guard.connection();
            let mut stmt = conn.prepare_cached(SQLITE_SELECT)?;
            for id in chunk {
                let row = stmt.query_row([id], BenchRow::from_rusqlite)?;
                // try to prevent compiler optimizing away the work we're timing
                black_box(row);
            }
            Ok::<(), rusqlite::Error>(())
        }));
    }

    for handle in handles {
        handle.await.expect("rusqlite blocking worker panicked")?;
    }

    Ok(())
}

fn benchmark_middleware_parallel_select(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;
    let config = MIDDLEWARE_CONFIG.clone();
    let concurrency = *BENCH_CONCURRENCY;

    group.bench_function(
        BenchmarkId::new("middleware_parallel_select", concurrency),
        |b| {
            let ids = ids.clone();
            let config = config.clone();
            b.to_async(runtime).iter_custom(move |iters| {
                let ids = ids.clone();
                let config = config.clone();
                async move {
                    let mut total = Duration::default();
                    for _ in 0..iters {
                        let start = Instant::now();
                        middleware_parallel_select(&config, &ids, concurrency)
                            .await
                            .expect("middleware parallel select");
                        total += start.elapsed();
                    }
                    total
                }
            });
        },
    );
}

fn benchmark_middleware_pool_checkout(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let config = MIDDLEWARE_CONFIG.clone();
    let concurrency = *BENCH_CONCURRENCY;

    group.bench_function(
        BenchmarkId::new("middleware_pool_checkout", concurrency),
        |b| {
            let config = config.clone();
            b.to_async(runtime).iter_custom(move |iters| {
                let config = config.clone();
                async move {
                    let mut total = Duration::default();
                    for _ in 0..iters {
                        let start = Instant::now();
                        middleware_parallel_checkout(&config, concurrency)
                            .await
                            .expect("middleware pool checkout");
                        total += start.elapsed();
                    }
                    total
                }
            });
        },
    );
}

fn benchmark_rusqlite_blocking(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;
    let concurrency = *BENCH_CONCURRENCY;

    group.bench_function(
        BenchmarkId::new("rusqlite_spawn_blocking", concurrency),
        |b| {
            let ids = ids.clone();
            let pool = BlockingRusqlitePool::new(dataset.path(), concurrency);
            b.to_async(runtime).iter_custom(move |iters| {
                let ids = ids.clone();
                let pool = pool.clone();
                async move {
                    let mut total = Duration::default();
                    for _ in 0..iters {
                        let start = Instant::now();
                        rusqlite_parallel_select(pool.clone(), &ids, concurrency)
                            .await
                            .expect("rusqlite parallel select");
                        total += start.elapsed();
                    }
                    total
                }
            });
        },
    );
}

fn sqlite_multithread_pool_checkout(c: &mut Criterion) {
    let dataset = &*DATASET;
    let lookup_count = dataset.ids().len() as u64;

    let mut group = c.benchmark_group("sqlite_multithread_pool_checkout");
    group.throughput(Throughput::Elements(lookup_count));

    benchmark_middleware_parallel_select(&mut group);
    benchmark_middleware_pool_checkout(&mut group);
    benchmark_rusqlite_blocking(&mut group);

    group.finish();
}

criterion_group!(benches, sqlite_multithread_pool_checkout);
criterion_main!(benches);
