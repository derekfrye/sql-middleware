# Issue 3: Transaction connection restoration ergonomics

- **Context:** README section "Custom logic in between transactions" (`docs/README.md`:220-369) shows an `if let Some(restored_conn)` block that reinserts the restored SQLite pooled connection after `run_prepared_with_finalize`.
- **Current behavior:** `run_prepared_with_finalize` captures any `TxOutcome` restored connection and stores it in a mutable `restored` slot; callers must copy it back into `MiddlewarePoolConnection` (only meaningful for SQLite, no-op for Postgres/Turso/LibSQL).
- **Pain point:** Call sites need SQLite-specific restoration glue (`if let Some(restored_conn) { *conn = restored_conn; }`), which breaks backend symmetry and adds boilerplate.
- **Refactor options (not implemented yet):**
  1. Change the transaction API to hold a mutable borrow of the pooled wrapper (e.g., a `PooledTx<'a>`). Commit/rollback/drop would rewrap internally, eliminating caller restoration branches.
  2. Provide a higher-level helper that accepts `&mut MiddlewarePoolConnection`, runs tx work, and performs the `TxOutcome` restoration internally (so call sites stay backend-agnostic without changing the lower-level API).
