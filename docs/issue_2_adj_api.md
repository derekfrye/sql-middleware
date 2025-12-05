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

## Typestate sketch (requested follow-up)

The follow-up request proposes a typestate wrapper so callers get compile-time states for “idle connection” vs “transaction” without abandoning deadpool or breaking the enum-based helpers. A possible shape:

```rust
use sql_middleware::typed::{Connection, Idle, InTx}; // new wrapper module
use sql_middleware::middleware::{ConfigAndPool, RowValues};

// User helper works in both modes (auto-commit or inside an existing tx).
async fn update_user(conn: &mut Connection<impl UpdatableState>) -> Result<(), SqlMiddlewareDbError> {
    conn.query("UPDATE users SET name = $1 WHERE id = $2")
        .params(&[RowValues::Text("New Name".into()), RowValues::Int(42)])
        .dml()
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), SqlMiddlewareDbError> {
    let cap = ConfigAndPool::sqlite_builder("file::memory:?cache=shared".to_string())
        .build()
        .await?;
    let mut conn: Connection<Idle> = Connection::from(cap.get_connection().await?);

    // Auto-commit path (matches today’s pooled-connection behavior).
    update_user(&mut conn).await?;

    // Explicit transaction path.
    let mut tx_conn: Connection<InTx> = conn.begin().await?; // consumes Idle, returns InTx
    update_user(&mut tx_conn).await?;
    // more work...
    conn = tx_conn.commit().await?; // or rollback()
    Ok(())
}
```

Notes on feasibility:
- Built as an opt-in layer over existing enums/Tx types; deadpool stays.
- `begin()` consumes `Connection<Idle>` and yields `Connection<InTx>`; `commit`/`rollback` consume `InTx` and return `Idle`.
- Commit/rollback semantics across backends must be normalized (likely consume the tx wrapper).
- Can ship alongside current enum-based APIs to avoid breakage; callers migrate gradually.

### Implementation plan (owned tx + typestate)
- Refactor backend transaction types to own their connections and use explicit `BEGIN/COMMIT/ROLLBACK`:
  - Postgres: replace `tokio_postgres::Transaction<'_>`-based `Tx` with an owned `deadpool_postgres::Object` wrapper issuing `BEGIN/COMMIT/ROLLBACK` and preparing/executing directly.
  - LibSQL/Turso: already use explicit statements; adjust to own the connection and consume on commit/rollback.
  - SQLite: already owns a cloned worker handle; normalize commit/rollback to consume and return to idle state in the wrapper.
  - MSSQL: mirror the owned approach with explicit transaction control.
- Normalize commit/rollback to consume the tx handle across backends so the typestate can transition back to `Idle`.
- Add a new `typed` module with `Connection<Idle>`/`Connection<InTx>` wrappers that move the owned connection into/out of the transaction state; expose `begin()/commit()/rollback()` on the wrapper.
- Keep existing enum-based helpers (`BatchTarget`/`QueryTarget`, `execute_batch`/`query`) for compatibility; optionally implement them atop the new owned tx types.
- Update docs/tests to cover the new typestate API; gate it behind an opt-in feature initially if needed.

### Risks and caveats
- Scope/complexity: cross-backend refactor plus a new API surface; significant testing needed across features.
- Behavioral drift: switching Postgres from `client.transaction()` to explicit `BEGIN/COMMIT` may differ in error modes/session semantics.
- User SQL containing txn control: once the wrapper issues `BEGIN`, user-issued `BEGIN/COMMIT` will error on PG and may behave oddly on SQLite/libsql/turso; recommend savepoints or document “no txn control inside managed tx.”
- Pool pressure: owned tx handles keep connections checked out longer; caller forgets to commit/rollback can starve the pool.
- Compatibility: even if the enum API stays, the new layer adds support burden; regressions possible during the refactor.
