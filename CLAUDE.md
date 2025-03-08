# SQL-Middleware Style Guide

## Build Commands
- Build: `cargo build`
- Test all: `cargo test`
- Test single: `cargo test test_name`
- Example: `cargo test test4_trait`

## Code Style

### Imports
- Group by source: std, external crates, local modules
- Use relative paths for local imports
- Example: `use crate::{postgres, sqlite};`

### Types
- Enums for type variants: `DatabaseType`, `RowValues`
- Error handling with thiserror: `#[derive(Error)]`
- Use async traits with `#[async_trait]`

### Naming
- Snake_case for functions/variables
- CamelCase for types/traits
- Types first, then implementations

### Error Handling
- Use Result<T, SqlMiddlewareDbError>
- Propagate errors with `?`
- Custom error type with transparent errors

### Documentation
- Include SQL file paths in test code
- Use descriptive variable names
- Maintain similar API between database backends

### Database API
- Consistent API for basic operations
- Database-specific API for transactions
- Use convert_sql_params for parameters