# SQLite rollback busy test hook isolation

## Context
- SQLite rollback can return `SQLITE_BUSY`; we have retry logic and a test hook to force that path.
- `tests/test12_sqlite_evict_when_busy` exercises the eviction vs rewrap behavior for rollback failures.

## Original behavior
- The test hook was a global `AtomicBool` (`set_force_rollback_busy_for_tests`) checked by `rollback_with_busy_retries`.
- Any test enabling it forced all SQLite rollbacks in the process to fail, so parallel runs (e.g., nextest) could leak the forced busy state into unrelated tests, even with in-memory shared-cache URLs.

## Fix
- Moved the hook to a per-connection flag stored on the `SqliteWorker` handle.
- `rollback_with_busy_retries` now consults the connection handle’s flag.
- Exposed a doc-hidden setter on `SqliteConnection` so tests can toggle the flag on specific pooled connections.
- Updated `tests/test12_sqlite_evict_when_busy` to set/clear the flag only on the connections it uses; no global resets required.

## Scope to other backends
- Postgres/MSSQL: server-side rollback, no `BUSY`-style shared-lock condition; pool drops broken connections on error paths.
- Turso/LibSQL: remote session errors surface as transport/session failures; no shared-memory busy state.
- No equivalent per-connection busy hook is needed today for those backends; if rollback failures ever leave a session poisoned, we would mark the pooled connection broken, similar to SQLite.

## Repro
- Run `cargo test --features sqlite --test test12_sqlite_evict_when_busy`; it forces rollback busy on its own connections to validate eviction vs rewrap behavior without affecting other tests.
