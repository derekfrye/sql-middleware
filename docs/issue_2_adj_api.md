# Top-level batch/query API options

Context: an issue asked for a single `execute_batch` that works with either a pooled connection or an explicit transaction, and a similar fluent `query` builder that can target both. Below are two viable designs for that API shape.

## 1) Enum-based targets (mirrors current dispatch style)
- Add a public enum `BatchTarget<'a>` (and similarly `QueryTarget<'a>`) with feature-gated variants for `&'a mut MiddlewarePoolConnection` and each backend transaction type (`PostgresTx`, `SqliteTx`, `MssqlTx`, `LibsqlTx`, `TursoTx`).
- Provide `From<&mut _>` impls so call sites can do `execute_batch((&mut conn).into(), sql).await?` or `execute_batch((&mut tx).into(), sql).await?`.
- Implement `pub async fn execute_batch(target: BatchTarget<'_>, sql: &str) -> Result<(), SqlMiddlewareDbError>` that matches the enum and delegates:
  - Connection variants call existing backend `execute_batch` helpers (which open/commit a transaction internally today).
  - Transaction variants call the transaction’s `execute_batch` directly (caller owns commit/rollback).
- For the builder, expose `pub fn query<'a>(target: impl Into<QueryTarget<'a>>, sql: &'a str) -> QueryBuilder<'a>`. The builder stores the enum and dispatches `select()`/`dml()` via `match`, reusing translation defaults per variant.
- Pros: keeps async fn signatures simple (no async-trait), stays consistent with existing pattern-matching dispatch. Cons: a new enum type surfaces in the public API.
- Example call (connection): `query((&mut conn).into(), sql).params(&params).select().await?`
  Example call (transaction): `query((&mut tx).into(), sql).params(&params).select().await?`

## 2) Trait-based targets (generic bound)
- Define a trait (e.g., `BatchCapable` and `QueryCapable`) implemented for `MiddlewarePoolConnection` and each backend transaction type. The trait exposes the small set of dispatch methods and translation defaults needed by `execute_batch`/`QueryBuilder`.
- Expose helpers like `pub async fn execute_batch(target: impl BatchCapable, sql: &str) -> Result<(), SqlMiddlewareDbError>` and `pub fn query<'a, T: QueryCapable + 'a>(target: &'a mut T, sql: &'a str) -> QueryBuilder<'a, 'a>`.
- Pros: slightly cleaner call sites (`execute_batch(&mut conn, sql).await?` and `query(&mut tx, sql)...`) without wrapping into an enum. Cons: needs an async-friendly trait solution (either `async-trait` or boxing) and is a style departure from current enum dispatch.
- Example call (connection): `query(&mut conn, sql).params(&params).select().await?`
  Example call (transaction): `query(&mut tx, sql).params(&params).select().await?`

## Shared behavioral notes
- Passing a pooled connection should keep the current behavior: the backend helper opens/commits a transaction around the work.
- Passing a transaction should not alter transaction boundaries; commit/rollback stays with the caller.
- Translation defaults must be derivable from the target (e.g., `translation_default` and `translation_target` helpers on the enum/trait).
- Docs should clarify the two pathways and the consequences for transaction ownership.
- In the initial enum implementation, transactions default translation to off; enable translation per call when you need placeholder rewriting in transactional code.

## interact_sync / interact_async and AnyConnWrapper
- Current `interact_async`/`interact_sync` are connection-only escape hatches that hand out `AnyConnWrapper` over raw clients. Neither proposed design conflicts with them; they remain as-is for direct/raw work that bypasses the builder/dispatch.
- If we want the unified targets to cover `interact_*`, we can add helper fns that accept a `BatchTarget`/`QueryTarget` and downcast to the underlying connection, but that’s optional and orthogonal to the batch/query helpers.
- `AnyConnWrapper` continues to work unchanged with either design; it’s still the adapter inside `interact_*`. We don’t need a transaction-aware `AnyConnWrapper` unless we decide to expose raw transaction handles through those helpers too.

## Perf and change size
- Perf: Enum dispatch is just a `match` on a small enum (no allocs/vtables). Trait dispatch is equivalent if monomorphized; if using `async-trait`/boxing, adds a tiny alloc + vtable per call—immaterial next to DB I/O but avoidable with enums.
- Change size: Enum approach is smaller/cleaner (new target enums, `From` impls, and a couple of matches). Trait approach touches more files to implement traits for each `Tx`, and either complicates async signatures or adds `async-trait`.

## README example mappings

- **Parameterized queries for reading or changing data**: Builder already used on a pooled connection. Enum design: `query((&mut conn).into(), &bound.query)...` keeps existing call chain; passing a transaction uses `(&mut tx).into()`. Trait design: `query(&mut conn, &bound.query)...` or `query(&mut tx, &bound.query)...`.

- **Multi-database support without copy/pasting query logic**: The backend-specific SQL `match` stays. Enum design: `query((&mut conn).into(), query).params(&params).select().await?`. Trait design: `query(&mut conn, query)...`. If a transaction is opened earlier, swap `conn` for `tx` without changing the builder chain.

- **Batch query w/o params**: Replace `conn.execute_batch(&ddl_query).await?` with unified entry: Enum design `execute_batch((&mut conn).into(), ddl_query).await?`; Trait design `execute_batch(&mut conn, ddl_query).await?`. If already in a transaction, pass `(&mut tx).into()` or `&mut tx` to avoid auto-commit.

- **Queries without parameters**: Same as builder examples; enum uses `query((&mut conn).into(), "SELECT * FROM users").select().await?`, trait uses `query(&mut conn, "...")...`. Transaction variants are interchangeable.

- **Custom logic in between transactions**: Instead of per-backend enums and manual dispatch, wrap the opened transaction in the unified target. Enum design: `let mut tx = begin_sqlite_tx(...)?; let rows = query((&mut tx).into(), base_query).translation(...).params(&dynamic_params).select().await?;` Caller still commits/rolls back. Trait design: same flow but pass `&mut tx` directly. Preparing statements can stay backend-specific, but the execute/dispatch surface can be unified via the target.

- **Using the query builder in helpers**: Helper signature can accept any target (`impl Into<QueryTarget<'_>>` or `impl QueryCapable`). Enum design call site: `insert_user((&mut conn).into(), ...).await?` and within helper `query(target, "...").translation(...).dml().await?`. Trait design: `insert_user(&mut conn, ...)` and `insert_user(&mut tx, ...)` both work if helper is generic over `QueryCapable`.
