#![cfg(feature = "turso")]

//! Criterion comparison of single-row SELECT latency for Turso via the
//! sql-middleware abstraction. Structured to mirror
//! `bench_rusqlite_single_row_lookup` so results stay comparable.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use sql_middleware::turso::{Params as TursoParams, build_result_set as turso_build_result_set};
use sql_middleware::{
    ConfigAndPool, ConversionMode, MiddlewarePoolConnection, ParamConverter, RowValues,
    SqlMiddlewareDbError,
};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use turso::Value as TursoValue;

/// Holds the reusable database path plus deterministic id workload.
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

// Dedicated runtime shared across async benchmarks.
static TOKIO_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("create tokio runtime"));

// Prepare a shared Turso dataset once so all benchmark variants hit identical data.
static DATASET: LazyLock<Dataset> = LazyLock::new(|| {
    let row_count = lookup_row_count_to_run();
    let path = PathBuf::from("benchmark_turso_single_lookup.db");
    TOKIO_RUNTIME
        .block_on(prepare_turso_dataset(&path, row_count))
        .expect("failed to prepare Turso dataset");

    let mut ids: Vec<i64> = lookup_ids(row_count);
    let mut rng = ChaCha8Rng::seed_from_u64(1_234_567_890);
    ids.shuffle(&mut rng);

    Dataset {
        path: path.to_string_lossy().into_owned(),
        ids,
    }
});

static MIDDLEWARE_CONFIG: LazyLock<ConfigAndPool> = LazyLock::new(|| {
    TOKIO_RUNTIME
        .block_on(ConfigAndPool::turso_builder(DATASET.path().to_string()).build())
        .expect("create Turso middleware pool")
});

static MIDDLEWARE_CONFIG_NO_CHECKOUT_TEST: LazyLock<ConfigAndPool> = LazyLock::new(|| {
    TOKIO_RUNTIME
        .block_on(
            ConfigAndPool::turso_builder(DATASET.path().to_string())
                .test_on_check_out(false)
                .build(),
        )
        .expect("create Turso middleware pool without checkout validation")
});

static DIRECT_TURSO_DB: LazyLock<Arc<turso::Database>> = LazyLock::new(|| {
    Arc::new(TOKIO_RUNTIME.block_on(async {
        turso::Builder::new_local(DATASET.path())
            .build()
            .await
            .expect("create direct Turso database handle")
    }))
});

static TRACE_MIDDLEWARE_QUERY: LazyLock<bool> =
    LazyLock::new(|| std::env::var("BENCH_TRACE").is_ok_and(|value| value != "0"));

static TURSO_SAMPLE_ROW: LazyLock<Arc<sql_middleware::CustomDbRow>> = LazyLock::new(|| {
    TOKIO_RUNTIME
        .block_on(async {
            let mut conn = MIDDLEWARE_CONFIG.get_connection().await?;
            let prepared = conn
                .prepare_turso_statement("SELECT id, name, score, active FROM test WHERE id = ?1")
                .await?;
            let params = [RowValues::Int(1)];
            let result = prepared.select().params(&params).all().await?;
            result.results.into_iter().next().ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError(
                    "sample row expected for middleware decode benchmark".to_string(),
                )
            })
        })
        .map(Arc::new)
        .expect("load sample Turso row for decode benchmark")
});

#[derive(Default)]
struct MiddlewareQueryBreakdown {
    total_query: Duration,
    total_decode: Duration,
    total_rows: u64,
    iterations: u64,
}

impl MiddlewareQueryBreakdown {
    fn record_iteration(&mut self) {
        self.iterations += 1;
    }

    fn record_row(&mut self, query: Duration, decode: Duration, rows_returned: usize) {
        self.total_query += query;
        self.total_decode += decode;
        self.total_rows += u64::try_from(rows_returned).expect("rows returned fits in u64");
    }

    fn report(&self) {
        if self.total_rows == 0 {
            return;
        }

        let total_rows = f64::from(u32::try_from(self.total_rows).expect("row count fits in u32"));
        let query_per_row = self.total_query.as_secs_f64() * 1_000_000_000.0 / total_rows;
        let decode_per_row = self.total_decode.as_secs_f64() * 1_000_000_000.0 / total_rows;

        eprintln!(
            "bench trace: middleware execute_select() {:.1} ns/row (decode {:.1} ns/row) across {} rows in {} iterations",
            query_per_row, decode_per_row, self.total_rows, self.iterations,
        );
    }
}

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

fn lookup_ids(row_count: usize) -> Vec<i64> {
    let row_count = i32::try_from(row_count).expect("benchmark row count fits in i32");
    (1..=row_count).map(i64::from).collect()
}

/// Create (or reset) a Turso database with predictable contents for repeatable runs.
async fn prepare_turso_dataset(path: &Path, row_count: usize) -> Result<(), SqlMiddlewareDbError> {
    if path.exists() {
        let _ = fs::remove_file(path);
        let owned = path.to_string_lossy().into_owned();
        let _ = fs::remove_file(format!("{owned}-wal"));
        let _ = fs::remove_file(format!("{owned}-shm"));
    }

    let config = ConfigAndPool::turso_builder(path.to_string_lossy().into_owned())
        .build()
        .await?;
    let mut conn = config.get_connection().await?;

    conn.execute_batch(
        "
        DROP TABLE IF EXISTS test;
        CREATE TABLE test (
            id      INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            score   REAL NOT NULL,
            active  INTEGER NOT NULL
        );
        BEGIN;
        ",
    )
    .await?;

    let row_count = i32::try_from(row_count).expect("benchmark row count fits in i32");
    for id in 1..=row_count {
        let db_id = i64::from(id);
        let params = [
            RowValues::Int(db_id),
            RowValues::Text(format!("name-{id}")),
            RowValues::Float(f64::from(id) * 0.5),
            RowValues::Bool(id % 2 == 0),
        ];
        let _ = conn
            .query("INSERT INTO test (id, name, score, active) VALUES (?1, ?2, ?3, ?4)")
            .params(&params)
            .dml()
            .await?;
    }

    conn.execute_batch("COMMIT;").await?;
    Ok(())
}

/// Compact struct used to ensure identical decoding cost across benchmarks.
#[derive(Debug)]
struct BenchRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

impl BenchRow {
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
            Some(RowValues::Int(value)) => {
                f64::from(i32::try_from(*value).expect("score fits in i32"))
            }
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

    fn from_turso_row(row: &turso::Row) -> Self {
        let id = match row
            .get_value(0)
            .expect("expected integer id column from turso row")
        {
            TursoValue::Integer(value) => value,
            other => panic!("unexpected id column type from turso row: {other:?}"),
        };

        let name = match row
            .get_value(1)
            .expect("expected text name column from turso row")
        {
            TursoValue::Text(text) => text,
            other => panic!("unexpected name column type from turso row: {other:?}"),
        };

        let score = match row
            .get_value(2)
            .expect("expected numeric score column from turso row")
        {
            TursoValue::Real(value) => value,
            TursoValue::Integer(value) => {
                f64::from(i32::try_from(value).expect("score fits in i32"))
            }
            other => panic!("unexpected score column type from turso row: {other:?}"),
        };

        let active = match row
            .get_value(3)
            .expect("expected boolean active column from turso row")
        {
            TursoValue::Integer(value) => value != 0,
            TursoValue::Real(value) => value != 0.0,
            TursoValue::Null => false,
            other => panic!("unexpected active column type from turso row: {other:?}"),
        };

        Self {
            id,
            name,
            score,
            active,
        }
    }

    fn into_tuple(self) -> (i64, String, f64, bool) {
        (self.id, self.name, self.score, self.active)
    }
}

/// Raw Turso baseline using a direct connection and cached statement.
fn benchmark_turso_raw(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;
    let db_handle = Arc::new(runtime.block_on(async {
        turso::Builder::new_local(dataset.path())
            .build()
            .await
            .expect("create raw Turso database handle")
    }));

    group.bench_function(BenchmarkId::new("turso_raw", ids.len()), |b| {
        let ids = ids.clone();
        let db_handle = db_handle.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let db_handle = db_handle.clone();
            async move {
                let mut total = Duration::default();
                let conn = db_handle.connect().expect("connect raw Turso database");
                let mut stmt = conn
                    .prepare("SELECT id, name, score, active FROM test WHERE id = ?1")
                    .await
                    .expect("prepare raw Turso statement");
                for _ in 0..iters {
                    let start = Instant::now();
                    for &id in &ids {
                        let mut rows = stmt.query([id]).await.expect("execute raw Turso select");
                        let row = rows
                            .next()
                            .await
                            .expect("fetch raw Turso row")
                            .expect("expected row from raw Turso select");
                        let bench_row = BenchRow::from_turso_row(&row);
                        black_box(bench_row);
                        while rows
                            .next()
                            .await
                            .expect("drain remaining raw Turso rows")
                            .is_some()
                        {}
                        stmt.reset().expect("reset raw Turso statement");
                    }
                    total += start.elapsed();
                }
                total
            }
        });
    });
}

/// Middleware benchmark that goes through the Turso prepared-statement helper.
/// should be similar to rusqlite `benchmark_middleware` in structure.
fn benchmark_middleware(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    benchmark_middleware_with_config(group, "middleware", MIDDLEWARE_CONFIG.clone());
    benchmark_middleware_with_config(
        group,
        "middleware_no_test_on_check_out",
        MIDDLEWARE_CONFIG_NO_CHECKOUT_TEST.clone(),
    );
}

fn benchmark_middleware_with_config(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &'static str,
    config_and_pool: ConfigAndPool,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;

    group.bench_function(BenchmarkId::new(name, ids.len()), |b| {
        let ids = ids.clone();
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let config_and_pool = config_and_pool.clone();
            let mut breakdown =
                TRACE_MIDDLEWARE_QUERY.then_some(MiddlewareQueryBreakdown::default());
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    if let Some(stats) = breakdown.as_mut() {
                        stats.record_iteration();
                    }
                    let mut conn = config_and_pool
                        .get_connection()
                        .await
                        .expect("acquire middleware connection");
                    let prepared = conn
                        .prepare_turso_statement(
                            "SELECT id, name, score, active FROM test WHERE id = ?1",
                        )
                        .await
                        .expect("prepare middleware statement");
                    let mut params = vec![RowValues::Int(0)];
                    let start = Instant::now();
                    for &id in &ids {
                        params[0] = RowValues::Int(id);
                        if let Some(stats) = breakdown.as_mut() {
                            let query_start = Instant::now();
                            let result = prepared
                                .select()
                                .params(&params)
                                .all()
                                .await
                                .expect("execute middleware select");
                            let query_elapsed = query_start.elapsed();

                            let decode_start = Instant::now();
                            let row = result.results.first().expect("expected row in result set");
                            let data = BenchRow::from_result_row(row);
                            black_box(data.into_tuple());
                            let decode_elapsed = decode_start.elapsed();

                            stats.record_row(query_elapsed, decode_elapsed, result.results.len());
                        } else {
                            let result = prepared
                                .select()
                                .params(&params)
                                .all()
                                .await
                                .expect("execute middleware select");
                            let row = result.results.first().expect("expected row in result set");
                            let data = BenchRow::from_result_row(row);
                            black_box(data.into_tuple());
                        }
                    }
                    total += start.elapsed();
                }

                if let Some(stats) = breakdown {
                    stats.report();
                }

                total
            }
        });
    });
}

/// Measure the cost of Turso's public `Database::connect()` path without the middleware pool.
fn benchmark_database_connect(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let db_handle = DIRECT_TURSO_DB.clone();
    let lookup_len = DATASET.ids().len();

    group.bench_function(BenchmarkId::new("database_connect", lookup_len), |b| {
        let db_handle = db_handle.clone();
        b.iter_custom(move |iters| {
            let db_handle = db_handle.clone();
            let mut total = Duration::default();
            for _ in 0..iters {
                let start = Instant::now();
                let conn = db_handle.connect().expect("connect direct Turso database");
                drop(conn);
                total += start.elapsed();
            }
            total
        });
    });

    group.bench_function(
        BenchmarkId::new("database_connect_async_shell", lookup_len),
        |b| {
            let db_handle = db_handle.clone();
            b.to_async(runtime).iter_custom(move |iters| {
                let db_handle = db_handle.clone();
                async move {
                    let mut total = Duration::default();
                    for _ in 0..iters {
                        let start = Instant::now();
                        let conn = db_handle.connect().expect("connect direct Turso database");
                        drop(conn);
                        total += start.elapsed();
                    }
                    total
                }
            });
        },
    );
}

/// Measure the cost of checking out and dropping a middleware connection.
fn benchmark_pool_acquire(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    benchmark_pool_acquire_with_config(group, "pool_acquire", MIDDLEWARE_CONFIG.clone());
    benchmark_pool_acquire_with_config(
        group,
        "pool_acquire_no_test_on_check_out",
        MIDDLEWARE_CONFIG_NO_CHECKOUT_TEST.clone(),
    );
}

fn benchmark_pool_acquire_with_config(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &'static str,
    config_and_pool: ConfigAndPool,
) {
    let runtime = &*TOKIO_RUNTIME;
    let lookup_len = DATASET.ids().len();

    group.bench_function(BenchmarkId::new(name, lookup_len), |b| {
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = config_and_pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    let conn = pool.get_connection().await.expect("checkout connection");
                    drop(conn);
                    total += start.elapsed();
                }
                total
            }
        });
    });
}

/// Measure the overhead of preparing a Turso statement through the middleware.
fn benchmark_middleware_prepare(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let config_and_pool = MIDDLEWARE_CONFIG.clone();
    let lookup_len = DATASET.ids().len();

    group.bench_function(BenchmarkId::new("middleware_prepare", lookup_len), |b| {
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = config_and_pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let mut conn = pool.get_connection().await.expect("checkout connection");
                    let start = Instant::now();
                    let prepared = conn
                        .prepare_turso_statement(
                            "SELECT id, name, score, active FROM test WHERE id = ?1",
                        )
                        .await
                        .expect("prepare statement");
                    total += start.elapsed();
                    drop(prepared);
                    drop(conn);
                }
                total
            }
        });
    });
}

/// Measure the overhead of a minimal middleware SELECT returning zero rows.
fn benchmark_middleware_interact_only(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let runtime = &*TOKIO_RUNTIME;
    let config_and_pool = MIDDLEWARE_CONFIG.clone();
    let lookup_len = DATASET.ids().len();

    group.bench_function(BenchmarkId::new("middleware_interact", lookup_len), |b| {
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let pool = config_and_pool.clone();
            async move {
                let mut total = Duration::default();
                let mut conn = pool.get_connection().await.expect("checkout connection");
                for _ in 0..iters {
                    let start = Instant::now();
                    let _ = conn
                        .query("SELECT 1 WHERE 0 = 1")
                        .select()
                        .await
                        .expect("execute noop select");
                    total += start.elapsed();
                }
                drop(conn);
                total
            }
        });
    });
}

/// Measure result-set materialisation using Turso helpers directly.
fn benchmark_middleware_marshalling(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;
    let config_and_pool = MIDDLEWARE_CONFIG.clone();

    group.bench_function(BenchmarkId::new("middleware_marshalling", ids.len()), |b| {
        let ids = ids.clone();
        let config_and_pool = config_and_pool.clone();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let config_and_pool = config_and_pool.clone();
            async move {
                let mut total = Duration::default();
                for _ in 0..iters {
                    let mut conn = config_and_pool
                        .get_connection()
                        .await
                        .expect("checkout connection");
                    if let MiddlewarePoolConnection::Turso {
                        conn: turso_conn, ..
                    } = &mut conn
                    {
                        for &id in &ids {
                            let mut stmt = turso_conn
                                .prepare("SELECT id, name, score, active FROM test WHERE id = ?1")
                                .await
                                .expect("prepare statement");
                            let cols = stmt.column_names();
                            let cols_arc = Arc::new(cols);
                            let params = [RowValues::Int(id)];
                            let converted = <TursoParams as ParamConverter>::convert_sql_params(
                                &params,
                                ConversionMode::Query,
                            )
                            .expect("convert params")
                            .0;
                            let start = Instant::now();
                            let rows = stmt.query(converted).await.expect("query turso statement");
                            let result = turso_build_result_set(rows, Some(cols_arc))
                                .await
                                .expect("build result set");
                            black_box(result);
                            total += start.elapsed();
                        }
                    }
                    drop(conn);
                }
                total
            }
        });
    });
}

/// Measure the cost of decoding a `CustomDbRow` into the bench struct.
fn benchmark_middleware_decode(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let ids = DATASET.ids().to_vec();

    group.bench_function(BenchmarkId::new("middleware_decode", ids.len()), |b| {
        let ids = ids.clone();
        let sample_row = TURSO_SAMPLE_ROW.clone();
        b.iter_custom(move |iters| {
            let ids = ids.clone();
            let sample_row = sample_row.clone();
            let mut total = Duration::default();
            for _ in 0..iters {
                for _ in &ids {
                    let start = Instant::now();
                    let data = BenchRow::from_result_row(&sample_row);
                    black_box(data.into_tuple());
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
                        let converted = <TursoParams as ParamConverter>::convert_sql_params(
                            &params,
                            ConversionMode::Query,
                        )
                        .expect("convert params");
                        black_box(converted);
                    }
                    total += start.elapsed();
                }
                total
            });
        },
    );
}

fn turso_single_row_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("turso_single_row_lookup");
    let lookup_count = DATASET.ids().len() as u64;
    group.throughput(Throughput::Elements(lookup_count));

    benchmark_turso_raw(&mut group);
    benchmark_middleware(&mut group);
    benchmark_database_connect(&mut group);
    benchmark_pool_acquire(&mut group);
    benchmark_middleware_prepare(&mut group);
    benchmark_middleware_interact_only(&mut group);
    benchmark_middleware_marshalling(&mut group);
    benchmark_middleware_decode(&mut group);
    benchmark_middleware_param_conversion(&mut group);

    group.finish();
}

criterion_group!(benches, turso_single_row_lookup);
criterion_main!(benches);
