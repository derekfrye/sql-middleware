# Long LOC Refactor Ideas

## src/pool/connection.rs
- Option 1: Split per-backend connection handling into `pool/connection/{postgres,sqlite,mssql,libsql,turso}.rs`, keeping the enum and helper traits thin in `connection.rs`. Each backend file owns its `get_connection` branch and connection-specific helpers (e.g., SQLite worker helpers).
- Option 2: Separate concerns by layer: a `pool/connection/types.rs` for the enum and Debug impl, `pool/connection/factory.rs` for checkout/connect logic, and `pool/connection/sqlite.rs` for SQLite worker-only helpers (`with_blocking_sqlite`, prepared-statement helpers).

## src/postgres/typed.rs
- Option 1: Carve by connection state: `typed/state_idle.rs`, `typed/state_tx.rs`, and a small `typed/mod.rs` that re-exports and holds shared helpers. Move type aliases and manager wiring into a `typed/context.rs`.
- Option 2: Split by capability: `typed/query.rs` (select/dml paths), `typed/batch.rs` (execute_batch/prepared re-use), and `typed/tx.rs` (transaction open/commit/rollback), leaving struct/trait definitions in `typed/core.rs`. **Implemented (done via core/select/dml/tx/prepared split).**

## src/sqlite/connection.rs
- Option 1: Break into `sqlite/connection/core.rs` (connection type, common helpers), `sqlite/connection/prepared.rs` (prepared statement lifecycle and worker handles), and `sqlite/connection/exec.rs` (select/dml/execute_batch entry points).
- Option 2: Divide by async boundary: `sqlite/connection/worker.rs` for blocking worker interactions (rusqlite calls, with_blocking helpers), `sqlite/connection/async_api.rs` for async-facing wrappers, and `sqlite/connection/types.rs` for enums/structs used by both.

## src/sqlite/typed.rs
- Option 1: Mirror postgres split: `typed/state_idle.rs`, `typed/state_tx.rs`, and `typed/core.rs` for shared types/aliases. Each state file owns its impl blocks.
- Option 2: Feature-focused: `typed/query.rs` (select/dml), `typed/prepared.rs` (statement prep/bind helpers), and `typed/tx.rs` (begin/commit/rollback, savepoints if any), keeping the connection struct and manager glue in `typed/mod.rs`. **Implemented (done via core/select/dml/tx/prepared split).**

## src/turso/typed.rs
- Option 1: State-centric split (`typed/state_idle.rs`, `typed/state_tx.rs`) with `typed/core.rs` holding connection structs, manager setup, and shared utilities.
- Option 2: Capability split similar to SQLite/Postgres: `typed/query.rs`, `typed/batch.rs` (bulk/execute_batch), `typed/tx.rs`, with a slim `typed/mod.rs` re-exporting the pieces. **Implemented (done via core/select/dml/tx/prepared split).**

## src/typed/any.rs
- Option 1: Split routing from type definitions: keep enums in `any/types.rs` and move the match-based dispatch impls (`Queryable`, `TypedConnOps`, `BeginTx`) into `any/dispatch.rs`.
- Option 2: Break by trait: `any/queryable.rs` for `Queryable` impls, `any/ops.rs` for `TypedConnOps` and tx helpers, leaving `any/mod.rs` or `any/types.rs` for the enums and conversions. **Implemented (done).**

## src/executor.rs
- Option 1: Separate targets from execution: `executor/targets.rs` for `BatchTarget`/`QueryTarget` and conversions, `executor/dispatch.rs` for `execute_*_dispatch` and per-backend arms, leaving `executor/mod.rs` as a thin facade. **Implemented (done).**
- Option 2: Backend shims: keep core traits/structs in `executor/mod.rs` and move backend-specific dispatch code into `executor/{postgres,sqlite,mssql,libsql,turso}.rs`, each exposing `execute_select`/`execute_dml` helpers to reduce the central match complexity.

## src/query_builder.rs
- Option 1: Extract translation concerns: keep the builder struct in `query_builder/core.rs`, move `translate_query_for_target` and placeholder handling into `query_builder/translation.rs`, and backend dispatch wiring into `query_builder/dispatch.rs`.
- Option 2: Split by operation: `query_builder/select.rs` and `query_builder/dml.rs` implementing the operation-specific methods, with `query_builder/mod.rs` holding the struct definition, parameter handling, and shared helpers. **Implemented (done).**

## src/translation.rs
- Option 1: Split types from algorithm: `translation/types.rs` for `PlaceholderStyle`, `TranslationMode`, `QueryOptions`, and `translation/engine.rs` for `translate_placeholders` and its state machine utilities.
- Option 2: Separate state machine pieces: keep public API in `translation/mod.rs`, move scanning helpers/state enums into `translation/scanner.rs`, and house dollar-quote/comment parsing in `translation/parsers.rs` to make the main function read shorter. **Implemented.**
