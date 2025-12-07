# DRY Violations – Consolidated Plan

Prioritized, deduplicated actions to reduce DRY issues across backends and typed adapters. Ordered from quickest/lowest risk to higher effort/risk. Keep feature flags and public API intact; add tests where behavior could shift.

1. **Extract column-name helper**
   - Backends: all (shared query utilities used across variants).
   - Add `extract_column_names()` (or similar) to shared query utilities to replace repeated `.map(|c| c.name().to_string())`.
   - Low risk; unit-test column name extraction for one backend to cover the helper.

2. **Standardize affected-rows conversion**
   - Backends: postgres, libsql, mssql.
   - Introduce backend-local `convert_affected_rows()` helpers (e.g., in `postgres/query.rs`, `libsql/executor.rs`, `mssql/query.rs`) for `u64 -> usize` handling.
   - Validate with existing affected-rows assertions; minimal change surface.

3. **Consolidate Turso prepare/execute select**
   - Backends: turso.
   - Create `prepare_and_execute_select()` in `query.rs` (or equivalent) and use in `executor.rs` and `transaction.rs`.
   - Add/adjust tests around Turso SELECT execution paths if coverage is light.

4. **Extract SQLite DML/SELECT helpers**
   - Backends: sqlite.
   - Add `execute_dml_internal()` and `execute_select_internal()` in `query.rs`, reused by query and transaction modules.
   - Run SQLite integration tests to confirm identical behavior.

5. **Macro for typed backend impls**
   - Backends: all typed backends (sqlite, postgres, libsql, mssql, turso).
   - Generate repeated trait impls in `src/typed/impl_*` via a `macro_rules!` template (Queryable, TypedConnOps, BeginTx, TxConn).
   - Guard invocations with `#[cfg(feature = ...)]`; add unit tests for one backend to ensure generated impls behave as before.

6. **Async wrapper macro**
   - Backends: all (shared pattern).
   - Replace repetitive `async move { ... }` wrapper patterns with a small macro (keep trait signatures stable).
   - Rely on clippy/tests to confirm no regressions; consider `async_trait` only if signatures allow.

7. **Dispatch arm consolidation**
   - Backends: all enum-dispatch sites (respect feature flags for postgres, sqlite, libsql, mssql, turso).
   - Add macro/helper for enum dispatch match arms that only differ by backend call, preserving `#[cfg]` guards.
   - Smoke-test across enabled backends; ensure compile-gates are respected.

8. **Standardize typed connection error handling**
   - Backends: postgres, sqlite, turso typed modules (extendable to others as needed).
   - Introduce shared trait/macro in typed module for “take connection / already taken” error paths across postgres/sqlite/turso typed modules.
   - Add a small unit test to assert consistent error text/variant.

9. **Unify Drop rollback + test helpers**
   - Backends: all typed modules.
   - Extract shared Drop rollback logic and SKIP_DROP_ROLLBACK pattern into typed utilities; invoke from all typed modules.
   - Add tests for best-effort rollback on drop and skip behavior.

10. **Clean up Idle vs InTx duplication (per backend)**
    - Backends: all, applied within each backend module where duplication exists.
    - Factor shared logic (execute_batch, dml, select, begin/commit/rollback wrappers) into backend-local helpers.
    - Re-run backend-specific integration tests to confirm no behavioral drift.

11. **Transaction wrapper shape extraction**
    - Backends: all (Tx/Prepared shapes across postgres, sqlite, libsql, mssql, turso).
    - Define a small trait/template capturing Tx/Prepared/execute/query shape; implement backend-specific internals.
    - Higher risk: ensure transactional semantics/tests stay intact across backends.

12. **Centralize parameter/result-set adapters**
    - Backends: all (with backend-specific hooks).
    - Build shared helpers for `RowValues -> driver params` and driver rows -> `ResultSet`, with backend hooks for internals.
    - Add focused tests comparing result-set output and parameter handling before/after.

13. **Auto-commit wrapper macro (optional)**
    - Backends: all typed modules (if pursued).
    - Evaluate macro for auto-commit pattern (take conn → begin → execute → commit → restore); proceed only if it reduces meaningful duplication without hiding behavior.
    - If adopted, add integration coverage for auto-commit flows per backend.

Testing guidance
- Run `cargo test` with relevant features (`sqlite,postgres,libsql,turso,mssql`) after each clustered change.
- Prefer small unit tests alongside helpers; keep integration tests for behavioral parity.
