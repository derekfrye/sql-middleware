# Repository Guidelines

## Project Structure & Module Organization
- Source: `src/` with feature-gated modules (`postgres/`, `sqlite/`, `libsql/`, `mssql/`) and core (`conversion.rs`, `executor.rs`, `middleware.rs`, `pool/`, `query.rs`, `results/`, `types.rs`).
- Public entry: `src/lib.rs` (forbids unsafe, re-exports key types; see `docs.md`).
- Tests: `tests/` (integration-style, e.g., `test03_sqlite.rs`, `test02_postgres.rs`).
- Benchmarks: `benches/` with Criterion; helper `bench.sh`.
- Docs: `docs/README.md` (symlinked as top-level `README.md`).

## Build, Test, and Development Commands
- Build: `cargo build` (add features via `--features "sqlite,postgres"`).
- Test (default features): `cargo test`.
- Run a specific test: `cargo test test03_sqlite -- --nocapture`.
- Benchmarks: `cargo bench` or `BENCH_ROWS=10000 cargo bench`; helper: `./bench.sh 10000`.
- Lint: `cargo clippy --all-targets --all-features -D warnings`.
- Format: `cargo fmt --all` (run before commits).

## Coding Style & Naming Conventions
- Edition: Rust 2024; `#![forbid(unsafe_code)]` — do not introduce `unsafe`.
- Use 4-space indentation, `snake_case` for modules/functions, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for consts.
- Prefer explicit errors over `unwrap()` in library code; `unwrap()` acceptable in tests/benches.
- Keep public API small and consistent across backends; add re-exports to `lib.rs` when appropriate.

## Testing Guidelines
- Framework: `cargo test` (async via `tokio`). Tests cover SQLite, PostgreSQL (embedded), and LibSQL.
- Naming: descriptive file names in `tests/` (e.g., `test_libsql.rs`). Group related cases in modules.
- Data files: SQLite tests may create `test_sqlite.db`; tests clean up when possible.
- Run with features as needed: `cargo test --features libsql,mssql`.

## Commit & Pull Request Guidelines
- Commits: short, imperative subject (e.g., "Refactor pool module"), focused changes.
- PRs: include summary, rationale, affected features (`postgres/sqlite/libsql/mssql`), and test/bench evidence. Link issues.
- Pre-submit: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, and update `docs/README.md` for user-facing API changes.

## Security & Configuration Tips
- Do not commit secrets. Use env vars for connection strings in examples.
- Feature flags control backends: defaults are `sqlite` and `postgres`; enable others with `--features`.
