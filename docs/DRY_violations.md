# DRY Violations – Consolidated Plan

Prioritized, deduplicated actions to reduce DRY issues across backends and typed adapters. Ordered from quickest/lowest risk to higher effort/risk. Keep feature flags and public API intact; add tests where behavior could shift.

Priority criteria (in order of weight):
- Smallest change surface area and lowest risk of behavioral drift.
- Backend-local helpers before cross-cutting refactors.
- Fewer call sites/feature gates before many.
- Tests already covering the behavior before changes that would require new tests.

1. **Extract column-name helper**
   - Backends: all (shared query utilities used across variants).
   - Estimated LOC change: ~20-40.
   - **Addressed:** Implemented `query_utils::extract_column_names` and updated Postgres/MSSQL/SQLite/Turso call sites to use it.
   - Add `extract_column_names()` (or similar) to shared query utilities to replace repeated `.map(|c| c.name().to_string())`.
   - Low risk; unit-test column name extraction for one backend to cover the helper.

2. **Standardize affected-rows conversion**
   - Backends: postgres, mssql.
   - Estimated LOC change: ~15-30.
   - **Addressed:** Added `convert_affected_rows` helpers in `postgres/query.rs` and `mssql/query.rs`, reused in executor/transaction paths.
   - Introduce backend-local `convert_affected_rows()` helpers (e.g., in `postgres/query.rs`, `mssql/query.rs`) for `u64 -> usize` handling.
   - Validate with existing affected-rows assertions; minimal change surface.

3. **Consolidate Postgres prepared vs direct helpers**
   - Backends: postgres.
   - Estimated LOC change: ~30-60.
   - **Addressed:** Added shared internal `execute_query_rows`/`execute_dml_rows` helpers in `postgres/query.rs` and reused in prepared/direct variants.
   - Factor shared logic between `execute_query_on_client` and `execute_query_prepared_on_client`, plus `execute_dml_on_client` and `execute_dml_prepared_on_client`.
   - Keep error messages and `build_result_set` usage identical; add a unit test if behavior changes.

4. **Consolidate Turso prepare/execute select**
   - Backends: turso.
   - Estimated LOC change: ~25-50.
   - Create `prepare_and_execute_select()` in `query.rs` (or equivalent) and use in `executor.rs` and `transaction.rs`.
   - Add/adjust tests around Turso SELECT execution paths if coverage is light.

5. **Extract SQLite DML/SELECT helpers**
   - Backends: sqlite.
   - Estimated LOC change: ~30-60.
   - Add `execute_dml_internal()` and `execute_select_internal()` in `query.rs`, reused by query and transaction modules.
   - Run SQLite integration tests to confirm identical behavior.

6. **Macro for typed backend impls**
   - Backends: all typed backends (sqlite, postgres, mssql, turso).
   - Estimated LOC change: ~80-150.
   - Generate repeated trait impls in `src/typed/impl_*` via a `macro_rules!` template (Queryable, TypedConnOps, BeginTx, TxConn).
   - Guard invocations with `#[cfg(feature = ...)]`; add unit tests for one backend to ensure generated impls behave as before.

7. **Async wrapper macro**
   - Backends: all (shared pattern).
   - Estimated LOC change: ~40-80.
   - Replace repetitive `async move { ... }` wrapper patterns with a small macro (keep trait signatures stable).
   - Rely on clippy/tests to confirm no regressions; consider `async_trait` only if signatures allow.

8. **Dispatch arm consolidation**
   - Backends: all enum-dispatch sites (respect feature flags for postgres, sqlite, mssql, turso).
   - Estimated LOC change: ~40-90.
   - Add macro/helper for enum dispatch match arms that only differ by backend call, preserving `#[cfg]` guards.
   - Smoke-test across enabled backends; ensure compile-gates are respected.

9. **Standardize typed connection error handling**
   - Backends: postgres, sqlite, turso typed modules (extendable to others as needed).
   - Estimated LOC change: ~30-60.
   - Introduce shared trait/macro in typed module for “take connection / already taken” error paths across postgres/sqlite/turso typed modules.
   - Add a small unit test to assert consistent error text/variant.

10. **Unify Drop rollback + test helpers**
   - Backends: all typed modules.
   - Estimated LOC change: ~50-100.
   - Extract shared Drop rollback logic and SKIP_DROP_ROLLBACK pattern into typed utilities; invoke from all typed modules.
   - Add tests for best-effort rollback on drop and skip behavior.

11. **Clean up Idle vs InTx duplication (per backend)**
   - Backends: all, applied within each backend module where duplication exists.
   - Estimated LOC change: ~60-120 per backend.
   - Factor shared logic (execute_batch, dml, select, begin/commit/rollback wrappers) into backend-local helpers.
   - Re-run backend-specific integration tests to confirm no behavioral drift.

12. **Transaction wrapper shape extraction**
   - Backends: all (Tx/Prepared shapes across postgres, sqlite, mssql, turso).
   - Estimated LOC change: ~90-180.
   - Define a small trait/template capturing Tx/Prepared/execute/query shape; implement backend-specific internals.
   - Higher risk: ensure transactional semantics/tests stay intact across backends.

13. **Centralize parameter/result-set adapters**
   - Backends: all (with backend-specific hooks).
   - Estimated LOC change: ~120-250.
   - Build shared helpers for `RowValues -> driver params` and driver rows -> `ResultSet`, with backend hooks for internals.
   - Add focused tests comparing result-set output and parameter handling before/after.

14. **Auto-commit wrapper macro (optional)**
   - Backends: all typed modules (if pursued).
   - Estimated LOC change: ~80-160.
   - Evaluate macro for auto-commit pattern (take conn → begin → execute → commit → restore); proceed only if it reduces meaningful duplication without hiding behavior.
   - If adopted, add integration coverage for auto-commit flows per backend.

Testing guidance
- Run `cargo test` with relevant features (`sqlite,postgres,turso,mssql`) after each clustered change.
- Prefer small unit tests alongside helpers; keep integration tests for behavioral parity.
