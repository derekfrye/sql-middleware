# Plan: DRY Refactoring for Turso typed.rs

This file has several instances of duplicated logic that can be consolidated. The main opportunities are: shared `conn_mut()` across states, unified `commit`/`rollback` via a helper, inline DML logic that duplicates the standalone function, and repeated error mapping patterns.

## Steps

1. Extract `conn_mut()` to a single generic `impl<State> TursoConnection<State>` block instead of duplicating it in both `Idle` and `InTx` impls.

2. Add a `finish_tx(sql, action)` helper to `TursoConnection<InTx>` (lines 127–147), then have `commit()` and `rollback()` call it (mirrors the Postgres pattern in `typed_postgres.rs`).

3. Have `TursoConnection<InTx>::dml` (lines 160–179) delegate to the standalone `dml()` function (lines 198–213) instead of duplicating the row-counting loop.

4. Create a helper like `turso_exec_error(context: &str) -> impl FnOnce(turso::Error) -> SqlMiddlewareDbError` to reduce the 9+ repeated `format!("turso ... error: {e}")` mappings.

5. Consolidate `take_conn()` and `take_conn_owned()` if feasible—both have identical error messages and differ only in `&mut self` vs `self`.

## Further Considerations

1. **Option wrapper necessity?** Postgres uses direct ownership without `Option`—do you want Turso to align, or keep the take/put pattern for error handling safety?

2. **Error helper scope:** Should the error helper be module-private or reused across other Turso submodules (e.g., `turso/query.rs`)?

## Summary Table

| Issue | Location | Severity | Suggested Fix |
|-------|----------|----------|--------------|
| Duplicate `conn_mut` | 2 impls | Medium | Generic impl on `TursoConnection<State>` |
| Duplicate `take_conn` variants | Idle + InTx | Low | Consolidate to shared method |
| Duplicate DML counting | InTx::dml + standalone dml | Medium | InTx::dml should call standalone |
| Repeated error mapping | ~9 locations | Medium | Error helper function |
| Duplicate commit/rollback structure | InTx impl | Medium | Add `finish_tx` helper (like Postgres) |
