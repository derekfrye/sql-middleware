# SQL-Middleware Style Guide

## Build & Test Commands
- Build: `cargo build`
- Check: `cargo clippy -- -W clippy::pedantic`
- Format: `cargo fmt`
- Test all: `cargo nextest run`
- Test single: `cargo nextest test_name` (e.g. `cargo nextest test_libsql`)
- Verbose test: `cargo nextest run -- --nocapture`
- Benchmarks: `cargo bench`

## Code Style

### Project Configuration
- Edition: 2024
- Default features: `["sqlite", "postgres", "mssql", "libsql"]`
- Optional feature: `test-utils` (includes postgresql_embedded for testing)

### Imports & Organization
- Group imports: std lib, external crates, local modules
- Example: `use crate::{postgres, sqlite, libsql};`
- Order: Types first, then implementations

### Types & Error Handling
- Enums for variants: `DatabaseType` (includes `Libsql`), `RowValues`
- Use `thiserror` with `#[derive(Error)]`
- Async traits with `#[async_trait]`
- Return `Result<T, SqlMiddlewareDbError>`
- Propagate errors with `?`
- Handle mutex poisoning gracefully (avoid `.unwrap()` on mutexes)

### Naming Conventions
- snake_case for functions/variables
- PascalCase for types/traits
- Descriptive variable names

### Database API
- Consistent interfaces between Postgres/SQLite/LibSQL/MSSQL
- Use `ParamConverter` trait with `convert_sql_params` for parameter conversion
- Support `ConversionMode` (Query vs Execute) for parameter handling
- Transaction handling with database-specific APIs
- `AnyConnWrapper` for unified connection handling
- Proper error propagation and handling

### LibSQL Support
- Local database: `ConfigAndPool::new_libsql(db_path)`
- Remote (Turso): `ConfigAndPool::new_libsql_remote(url, auth_token)`
- Uses `deadpool-libsql` for connection pooling
- Supports all standard operations (SELECT, DML, batch)