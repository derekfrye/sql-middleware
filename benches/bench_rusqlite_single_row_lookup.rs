//! Criterion comparison of single-row SELECT latency for raw `rusqlite` vs. the
//! sql-middleware abstraction. Each iteration reuses the same seeded dataset so
//! we focus on call overhead instead of storage effects.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rusqlite::{Connection, Row, params};
use sql_middleware::benchmark::sqlite::clean_sqlite_tables;
use sql_middleware::sqlite::{self, build_result_set};
use sql_middleware::{
    AsyncDatabaseExecutor, ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection, RowValues,
    SqlMiddlewareDbError,
};
use std::cell::RefCell;
use std::fs;
use std::hint::black_box;
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
    let row_count = lookup_row_count_to_run();
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

static MIDDLEWARE_CONFIG: LazyLock<ConfigAndPool> = LazyLock::new(|| {
    TOKIO_RUNTIME
        .block_on(ConfigAndPool::new_sqlite(DATASET.path().to_string()))
        .expect("create middleware pool")
});

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
        .unwrap_or(1000)
}

/// Create a fresh SQLite file with predictable contents for repeatable runs.
fn prepare_sqlite_dataset(path: &Path, row_count: usize) -> rusqlite::Result<()> {
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
#[allow(dead_code)]
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
    let config_and_pool = MIDDLEWARE_CONFIG.clone();

    group.bench_function(BenchmarkId::new("middleware", ids.len()), |b| {
        let ids = ids.clone();
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let config_and_pool = config_and_pool.clone();
            async move {
                let mut total = Duration::default();
                let query = "SELECT id, name, score, active FROM test WHERE id = ?1";
                let mut params = vec![RowValues::Int(0)];
                for _ in 0..iters {
                    clean_sqlite_tables(&config_and_pool)
                        .await
                        .expect("reset sqlite tables");
                    let pool = config_and_pool.pool.clone();
                    let mut conn = MiddlewarePool::get_connection(&pool)
                        .await
                        .expect("acquire middleware connection");
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

/// Measure the cost of checking out and dropping a middleware connection.
fn benchmark_middleware_checkout(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let config_and_pool = MIDDLEWARE_CONFIG.clone();
    let lookup_len = DATASET.ids().len();

    group.bench_function(BenchmarkId::new("middleware_checkout", lookup_len), |b| {
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = config_and_pool.pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    let conn = MiddlewarePool::get_connection(&pool)
                        .await
                        .expect("checkout connection");
                    drop(conn);
                    total += start.elapsed();
                }
                total
            }
        });
    });
}

/// Measure the overhead of the `interact` hop without executing a query.
fn benchmark_middleware_interact_only(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let config_and_pool = MIDDLEWARE_CONFIG.clone();
    let lookup_len = DATASET.ids().len();

    group.bench_function(BenchmarkId::new("middleware_interact", lookup_len), |b| {
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = config_and_pool.pool.clone();
            async move {
                let mut total = Duration::default();
                let mut conn = MiddlewarePool::get_connection(&pool)
                    .await
                    .expect("checkout connection");
                for _ in 0..iters {
                    let start = Instant::now();
                    if let MiddlewarePoolConnection::Sqlite(sqlite_conn) = &mut conn {
                        sqlite_conn
                            .interact(|_| Ok::<_, SqlMiddlewareDbError>(()))
                            .await
                            .expect("interact")
                            .expect("interact inner");
                    }
                    total += start.elapsed();
                }
                drop(conn);
                total
            }
        });
    });
}

/// Measure result-set materialisation using `build_result_set` directly.
fn benchmark_middleware_marshalling(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let path = dataset.path().to_string();

    group.bench_function(BenchmarkId::new("middleware_marshalling", ids.len()), |b| {
        let ids = ids.clone();
        let path = path.clone();
        b.iter_custom(move |iters| {
            let conn = Connection::open(&path).expect("open sqlite connection");
            let mut total = Duration::default();
            for _ in 0..iters {
                for &id in &ids {
                    let mut stmt = conn
                        .prepare("SELECT id, name, score, active FROM test WHERE id = ?1")
                        .expect("prepare statement");
                    let params =
                        sql_middleware::sqlite::params::convert_params(&[RowValues::Int(id)]);
                    let start = Instant::now();
                    let result = build_result_set(&mut stmt, &params).expect("build result set");
                    black_box(result);
                    total += start.elapsed();
                }
            }
            total
        });
    });
}

/// Measure parameter conversion cost for the middleware.
fn benchmark_middleware_param_conversion(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let ids = DATASET.ids().to_vec();

    group.bench_function(
        BenchmarkId::new("middleware_param_convert", ids.len()),
        |b| {
            let ids = ids.clone();
            b.iter_custom(move |iters| {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    for &id in &ids {
                        let params = [RowValues::Int(id)];
                        let converted = sql_middleware::sqlite::params::convert_params(&params);
                        black_box(converted);
                    }
                    total += start.elapsed();
                }
                total
            });
        },
    );
}

fn sqlite_single_row_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_single_row_lookup");
    let lookup_count = DATASET.ids().len() as u64;
    group.throughput(Throughput::Elements(lookup_count));

    benchmark_rusqlite_direct(&mut group);
    benchmark_middleware(&mut group);
    benchmark_middleware_checkout(&mut group);
    benchmark_middleware_interact_only(&mut group);
    benchmark_middleware_marshalling(&mut group);
    benchmark_middleware_param_conversion(&mut group);

    group.finish();
}

criterion_group!(benches, sqlite_single_row_lookup);
criterion_main!(benches);
