//! Criterion comparison of single-row SELECT latency for raw `rusqlite` vs. the
//! sql-middleware abstraction. Each iteration reuses the same seeded dataset so
//! we focus on call overhead instead of storage effects.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rusqlite::{Connection, Row, params};
use sql_middleware::{AsyncDatabaseExecutor, ConfigAndPool, MiddlewarePool, RowValues};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Holds the reusable on-disk database path plus deterministic id workload.
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

// Prepare a shared SQLite file once so both benchmark variants hit identical data.
static DATASET: LazyLock<Dataset> = LazyLock::new(|| {
    let row_count = lookup_row_count();
    let path = PathBuf::from("benchmark_sqlite_single_lookup.db");
    prepare_sqlite_dataset(&path, row_count).expect("failed to prepare SQLite dataset");

    let mut ids: Vec<i64> = (1..=row_count as i64).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(1_234_567_890);
    ids.shuffle(&mut rng);

    Dataset {
        path: path.to_string_lossy().into_owned(),
        ids,
    }
});

// Dedicated runtime for the async middleware path.
static TOKIO_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("create tokio runtime"));

/// Resolve how many lookups each iteration should perform.
fn lookup_row_count() -> usize {
    std::env::var("BENCH_LOOKUPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            std::env::var("BENCH_ROWS")
                .ok()
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(1000)
}

/// Create a fresh SQLite file with predictable contents for repeatable runs.
fn prepare_sqlite_dataset(path: &Path, row_count: usize) -> rusqlite::Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }

    let conn = Connection::open(path)?;
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

    let transaction = conn.transaction()?;
    {
        let mut insert_stmt = transaction
            .prepare("INSERT INTO test (id, name, score, active) VALUES (?1, ?2, ?3, ?4)")?;

        for id in 1..=row_count as i64 {
            let name = format!("name-{id}");
            let score = id as f64 * 0.5;
            let active = id % 2 == 0;
            insert_stmt.execute(params![id, name, score, active])?;
        }
    }
    transaction.commit()?;

    Ok(())
}

/// Compact struct used in both benchmark variants to ensure identical decoding cost.
#[derive(Debug)]
struct BenchRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

impl BenchRow {
    fn from_rusqlite(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            score: row.get(2)?,
            active: row.get(3)?,
        })
    }

    fn from_result_row(row: &sql_middleware::CustomDbRow) -> Self {
        let id = match row.get_by_index(0) {
            Some(RowValues::Int(value)) => *value,
            _ => panic!("expected integer id column"),
        };

        let name = match row.get_by_index(1) {
            Some(RowValues::Text(text)) => text.clone(),
            _ => panic!("expected text name column"),
        };

        let score = match row.get_by_index(2) {
            Some(RowValues::Float(value)) => *value,
            Some(RowValues::Int(value)) => *value as f64,
            _ => panic!("expected numeric score column"),
        };

        let active = match row.get_by_index(3) {
            Some(RowValues::Bool(value)) => *value,
            Some(RowValues::Int(value)) => *value != 0,
            _ => panic!("expected boolean active column"),
        };

        Self {
            id,
            name,
            score,
            active,
        }
    }
}

/// Raw `rusqlite` baseline using a cached prepared statement on a single connection.
fn benchmark_rusqlite_direct(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let connection = Connection::open(dataset.path()).expect("open sqlite connection");
    let statement = connection
        .prepare_cached("SELECT id, name, score, active FROM test WHERE id = ?1")
        .expect("prepare select statement");
    let statement = Rc::new(RefCell::new(statement));

    group.bench_function(BenchmarkId::new("rusqlite", ids.len()), |b| {
        let ids = ids.clone();
        let statement = statement.clone();
        b.iter_custom(move |iters| {
            let mut total = Duration::default();
            for _ in 0..iters {
                let mut stmt = statement.borrow_mut();
                let start = Instant::now();
                for &id in &ids {
                    let row = stmt
                        .query_row([id], |row| BenchRow::from_rusqlite(row))
                        .expect("query row");
                    black_box(row);
                }
                total += start.elapsed();
            }
            total
        });
    });
}

/// Middleware variant that goes through `MiddlewarePoolConnection::execute_select`.
fn benchmark_middleware(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;

    let config_and_pool = runtime
        .block_on(ConfigAndPool::new_sqlite(dataset.path().to_string()))
        .expect("create middleware pool");
    let middleware_pool = config_and_pool.pool.clone();

    group.bench_function(BenchmarkId::new("middleware", ids.len()), |b| {
        let ids = ids.clone();
        let pool = middleware_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let pool = pool.clone();
            async move {
                let mut conn = MiddlewarePool::get_connection(&pool)
                    .await
                    .expect("acquire middleware connection");
                let mut total = Duration::default();
                let query = "SELECT id, name, score, active FROM test WHERE id = ?1";
                let mut params = vec![RowValues::Int(0)];

                for _ in 0..iters {
                    let start = Instant::now();
                    for &id in &ids {
                        params[0] = RowValues::Int(id);
                        let result = conn
                            .execute_select(query, &params)
                            .await
                            .expect("execute middleware select");
                        let row = result.results.first().expect("expected row in result set");
                        let data = BenchRow::from_result_row(row);
                        black_box(data);
                    }
                    total += start.elapsed();
                }

                total
            }
        });
    });
}

fn sqlite_single_row_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_single_row_lookup");
    let lookup_count = DATASET.ids().len() as u64;
    group.throughput(Throughput::Elements(lookup_count));

    benchmark_rusqlite_direct(&mut group);
    benchmark_middleware(&mut group);

    group.finish();
}

criterion_group!(benches, sqlite_single_row_lookup);
criterion_main!(benches);
