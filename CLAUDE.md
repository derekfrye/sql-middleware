# SQL-Middleware Style Guide

## Build & Test Commands
- Build: `cargo build`
- Check: `cargo clippy`
- Format: `cargo fmt`
- Test all: `cargo test`
- Test single: `cargo test test_name` (e.g. `cargo test test2_postgres`)
- Verbose test: `cargo test -- --nocapture`

## Code Style

### Imports & Organization
- Group imports: std lib, external crates, local modules
- Example: `use crate::{postgres, sqlite};`
- Order: Types first, then implementations

### Types & Error Handling
- Enums for variants: `DatabaseType`, `RowValues`
- Use `thiserror` with `#[derive(Error)]`
- Async traits with `#[async_trait]`
- Return `Result<T, SqlMiddlewareDbError>`
- Propagate errors with `?`

### Naming Conventions
- snake_case for functions/variables
- PascalCase for types/traits
- Descriptive variable names

### Database API
- Consistent interfaces between Postgres/SQLite
- Use `convert_sql_params` for parameter conversion
- Transaction handling with database-specific APIs
- `QueryAndParams` for typed query parameters
- Proper error propagation and handling