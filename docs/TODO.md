# Refactor TODOs

Backend Duplication Hotspots
- Repeated `ConfigAndPool::new_*` constructors across backends (`src/sqlite/config.rs`, `src/postgres/config.rs`, `src/mssql/config.rs`, `src/turso/config.rs`) follow the same pool-init + smoke-test pattern with backend-specific wiring.
- Execution helpers (`execute_batch`, `execute_select`, `execute_dml`) share nearly identical control flow in `src/*/executor.rs`, differing mostly in adapter calls and error wording.
- Parameter conversion layers duplicate the mapping from `RowValues` into driver-native types and timestamp formatting logic in `src/*/params.rs`.
- Result-set builders mirror each other when walking columns/rows to populate `ResultSet` (`src/*/query.rs`).
- Transaction wrappers for SQLite-like engines (`src/sqlite/transaction.rs`, `src/turso/transaction.rs`) expose the same BEGIN/COMMIT/ROLLBACK and prepared-statement surface.
- Module scaffolding (`src/*/mod.rs`) re-exports the same API sets with only backend names changed.

Proposed Next Steps
- Extract shared traits/helpers for pool creation and CRUD execution paths so backends only provide driver-specific pieces (e.g., pool builder + type aliases).
- Centralise `RowValues` conversion and timestamp formatting into reusable helpers to avoid diverging behaviour between adapters.
- Consolidate result-set assembly into backend-agnostic utilities (parameterised by column/value extractor) to trim repeated loops.
- Explore a lightweight macro or template to cut down on the identical `mod.rs` re-export boilerplate per backend.

## Perf ideas

### Hot session / batch worker hop API

We already have with_blocking_sqlite, which is powerful but raw. A more structured safe wrapper could let users run a
     whole hot loop in one worker hop while still using middleware-ish types:

     conn.sqlite_hot_session(|session| {
         let stmt = session.prepare_cached(SQL)?;
         for id in ids {
             ...
         }
     }).await?

     This is less ergonomic than normal middleware calls, but it directly attacks worker-hop overhead for tight loops.
