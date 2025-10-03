use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sqlx::{ConnectOptions, FromRow, sqlite::{SqliteConnectOptions, SqlitePoolOptions}};
use std::hint::black_box;
use std::path::Path;
use std::str::FromStr;
use tempfile::{Builder, TempPath};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct BenchRow {
    id: i64,
    name: String,
    score: f64,
    active: bool,
}

struct Dataset {
    _temp_path: TempPath,
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

static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| Runtime::new().expect("create tokio runtime"));

static DATASET: LazyLock<Dataset> = LazyLock::new(|| {
    TOKIO_RUNTIME.block_on(async {
        let row_count = lookup_row_count_to_run();
        let temp_file = Builder::new()
            .prefix("sqlx_lookup")
            .suffix(".db")
            .tempfile()
            .expect("create temp sqlite file");
        let temp_path = temp_file.into_temp_path();
        prepare_sqlite_dataset(temp_path.as_ref(), row_count)
            .await
            .expect("failed to prepare dataset");

        let mut ids: Vec<i64> = (1..=row_count as i64).collect();
        let mut rng = ChaCha8Rng::seed_from_u64(1_234_567_890);
        ids.shuffle(&mut rng);

        let path_string = temp_path.to_string_lossy().into_owned();

        Dataset {
            _temp_path: temp_path,
            path: path_string,
            ids,
        }
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
        .unwrap_or(1000)
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

fn benchmark_sqlx_query_as(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;

    group.bench_function(BenchmarkId::new("sqlx", ids.len()), |b| {
        let ids = ids.clone();
        let path = dataset.path().to_string();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let path = path.clone();
            async move {
                let options = SqliteConnectOptions::from_str(&path)
                    .expect("options")
                    .create_if_missing(true)
                    .disable_statement_logging();

                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect_with(options)
                    .await
                    .expect("create sqlx pool");

                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    for &id in &ids {
                        let row: BenchRow = sqlx::query_as(
                            "SELECT id, name, score, active FROM test WHERE id = ?1",
                        )
                        .bind(id)
                        .fetch_one(&pool)
                        .await
                        .expect("sqlx fetch");
                        std::hint::black_box(row);
                    }
                    total += start.elapsed();
                }

                pool.close().await;

                total
            }
        });
    });

}

fn benchmark_sqlx_pool_acquire(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let runtime = &*TOKIO_RUNTIME;
    let lookup_len = dataset.ids().len();

    group.bench_function(BenchmarkId::new("sqlx_pool_acquire", lookup_len), |b| {
        let path = dataset.path().to_string();
        b.to_async(runtime).iter_custom(move |iters| {
            let path = path.clone();
            async move {
                let options = SqliteConnectOptions::from_str(&path)
                    .expect("options")
                    .create_if_missing(true)
                    .disable_statement_logging();
                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect_with(options)
                    .await
                    .expect("create sqlx pool");

                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    let conn = pool.acquire().await.expect("acquire connection");
                    drop(conn);
                    total += start.elapsed();
                }

                pool.close().await;

                total
            }
        });
    });
}

fn benchmark_sqlx_query_raw(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let dataset = &*DATASET;
    let ids = dataset.ids().to_vec();
    let runtime = &*TOKIO_RUNTIME;

    group.bench_function(BenchmarkId::new("sqlx_query_raw", ids.len()), |b| {
        let ids = ids.clone();
        let path = dataset.path().to_string();
        b.to_async(runtime).iter_custom(move |iters| {
            let ids = ids.clone();
            let path = path.clone();
            async move {
                let options = SqliteConnectOptions::from_str(&path)
                    .expect("options")
                    .create_if_missing(true)
                    .disable_statement_logging();
                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect_with(options)
                    .await
                    .expect("create sqlx pool");

                let mut total = Duration::default();
                for _ in 0..iters {
                    let start = Instant::now();
                    for &id in &ids {
                        let row = sqlx::query("SELECT id, name, score, active FROM test WHERE id = ?1")
                            .bind(id)
                            .fetch_one(&pool)
                            .await
                            .expect("sqlx fetch");
                        std::hint::black_box(row);
                    }
                    total += start.elapsed();
                }

                pool.close().await;

                total
            }
        });
    });
}

fn benchmark_sqlx_param_bind(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
) {
    let ids = DATASET.ids().to_vec();
    let lookup_len = ids.len();

    group.bench_function(BenchmarkId::new("sqlx_param_bind", lookup_len), |b| {
        let ids = ids.clone();
        b.iter_custom(move |iters| {
            let mut total = Duration::default();
            for _ in 0..iters {
                let start = Instant::now();
                for &id in &ids {
                    let query = sqlx::query("SELECT id, name, score, active FROM test WHERE id = ?1");
                    let bound = query.bind(id);
                    std::hint::black_box(bound);
                }
                total += start.elapsed();
            }
            total
        });
    });
}

fn sqlite_single_row_lookup_sqlx(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite_single_row_lookup_sqlx");
    group.throughput(Throughput::Elements(DATASET.ids().len() as u64));

    benchmark_sqlx_query_as(&mut group);
    benchmark_sqlx_query_raw(&mut group);
    benchmark_sqlx_pool_acquire(&mut group);
    benchmark_sqlx_param_bind(&mut group);

    group.finish();
}

criterion_group!(benches, sqlite_single_row_lookup_sqlx);
criterion_main!(benches);
