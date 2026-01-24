# Simulator Migration Plan

## Goal
Evolve the existing `simulator/` crate into a deterministic, property-driven simulator that tests the full sql-middleware stack across all backends, borrowing the design ideas from Turso's simulator without importing `turso_core` or Turso internals.

## Current State (sql-middleware simulator)
- Deterministic, single-threaded scheduler with a fake clock.
- Models pool checkout/return, tx begin/commit/rollback, and coarse fault injection.
- Oracle checks pool invariants only.
- Backend adapters drive real `sql-middleware` pools for SQLite; Postgres/Turso adapters exist behind feature flags.
- CLI includes plan/property generation and backend selection flags (see below).

## Concepts to Borrow from Turso
- Interaction plans: sequences of actions + assertions.
- Properties: named invariants that define required outcomes.
- Generation: random plan generation controlled by workload weights.
- Profiles: reusable bundles of weights, sizes, and error injection knobs.
- Differential/Doublecheck modes: run the same plan against multiple backends or twice to verify stability.
- Shrinking: reduce failing plans to a minimal counterexample.
- Bug base: optional storage of failing seeds/plans.

## Turso-Derived Properties and Oracles (Mapped to sql-middleware)
This is a concise mapping of existing Turso simulator properties/oracles into what translates cleanly for a pool/connection-focused simulator.

### Properties (Direct or Near-Term Fits)
- **InsertValuesSelect (PQS)** -> exercise write then read through pooled connections; validate end-to-end query/param plumbing and result shape.
- **ReadYourUpdatesBack** -> validate update visibility on the same connection; combine with tx boundaries to check commit/rollback visibility.
- **DeleteSelect** -> ensure deleted rows are not visible (in and out of a transaction) across pooled connections.
- **SelectLimit** -> deterministic result size checks with normalized ordering and row counts.
- **DoubleCreateFailure / DropSelect** -> normalize error class mapping and verify consistent error handling across backends.

### Properties (Defer or Optional)
- **SelectSelectOptimizer (NoREC)**, **WhereTrueFalseNull (TLP)**, **UnionAllPreservesCardinality** -> SQL-semantics-heavy; only include if we build robust query generation + result normalization.
- **TableHasExpectedContent / AllTableHaveExpectedContent** -> require a shadow model; consider replacing with differential oracle checks.
- **FsyncNoWait** -> storage semantics; translate to fault injection on connections instead (disconnect/reopen/busy/timeout).
- **FaultyQuery** -> use for pool/connection fault injection and error class normalization.

### Oracles (Core Modes to Implement)
- **Property** -> primary mode for pool/tx/connection lifecycle properties.
- **Differential** -> run the same plan against two backends (e.g., sqlite vs turso) and compare results/errors.
- **Doublecheck** -> run the same plan twice against one backend to verify deterministic behavior.
- **Shadow-state** -> optional; likely not needed for initial middleware focus.

## Target Architecture (New Simulator Design)
Delete the existing crate (its in .git so we can always backtrack later if needed) and refactor into clearly separated layers:

1) Plan + Properties
- Define a `Plan` type: `Vec<Interaction>`.
- `Interaction` includes:
  - Operation (connect, checkout, begin, execute, commit, rollback, sleep, etc.).
  - Optional SQL payload + parameter values.
  - Expected outcomes (assertions).
- `Property` enum defines reusable invariants (pool correctness, tx isolation, retry behavior, result shape).

2) Generation
- `generation/` module produces `Plan` from:
  - Workload distribution (reads/writes/ddl/tx).
  - Backend capabilities.
  - Property selection.
- Keep randomization deterministic via seed.

3) Runner
- `runner/` executes a `Plan` through a backend adapter.
- Collects a trace and checks assertions.
- Supports `run_once`, `loop`, `doublecheck`, and `differential` modes.

4) Backends
- Define a `Backend` trait with the minimum surface needed by the plan runner:
  - `open_pool`, `checkout`, `return_conn`
  - `begin`, `commit`, `rollback`
  - `execute(sql, params)`, `query(sql, params)`
  - `sleep(ms)`
  - `fault_inject` hooks (optional)
- Implement adapters for:
  - SQLite (rusqlite)
  - Postgres (tokio-postgres)
  - MSSQL (tiberius)
  - Turso (turso crate)
- Keep adapters in `simulator/src/backends/` with feature gates.

5) Oracle + Assertions
- Keep pool invariants but expand to:
  - Result-set invariants (row counts, deterministic ordering when specified).
  - Transaction invariants (commit visibility, rollback visibility).
  - Error invariants (busy/timeout/error class mapping).
- Assertions stored alongside interactions.

6) Shrink + Replay
- Store failing plans as JSON.
- Provide a shrinker that removes interactions and replays to find a minimal repro.
- Keep `--seed` reproducibility.

## Concrete Migration Steps

### Phase 0: Baseline Cleanup (1-2 days)
- Move current model-based simulator into `legacy/`.
- Add `docs/Simulator_plan.md` (this doc).
- Add `simulator/src/plan.rs` with minimal `Plan` + `Interaction` types.
- Add JSON serialization for plans (serde).

### Phase 1: Core Plan Runner (2-4 days)
- Implement `runner/mod.rs` with:
  - `run_plan(plan, backend, oracle, config)`
  - structured trace events.
- Adapt current scheduler to drive plan execution deterministically.
- Add plan logging:
  - JSON plan dump on failure.
  - human-readable trace summary (first N, last N steps).

### Phase 2: Properties + Generation (3-6 days)
- Introduce `properties/` with a minimal set. **Implemented** in `simulator/src/properties/mod.rs`:
  - `PoolCheckoutReturn`
  - `TxCommitVisible`
  - `TxRollbackInvisible`
  - `RetryAfterBusy`
  - Example: `cargo run -p simulator -- --property tx-rollback-invisible`
  - Choices/tradeoffs:
    - `RetryAfterBusy` uses `BEGIN IMMEDIATE` and expects a `locked`-matching error string; this is SQLite-specific and assumes pool size >= 2.
- Create `generation/` to build plans from workload weights and property selection. **Implemented** in `simulator/src/generation/mod.rs`.
  - Examples:
    - `cargo run -p simulator -- --generate --steps 500 --seed 42 --tasks 8`
    - `cargo run -p simulator -- --generate --property tx-commit-visible --steps 200`
  - Choices/tradeoffs:
    - The generator prepends a bootstrap table and optional property plan, then fills remaining steps with weighted actions.
    - Generated queries do not carry expectations yet; they are used to exercise the stack rather than assert results.
    - SQLite generation avoids non-transaction actions while any task has an open transaction to reduce lock errors in the single-threaded runner.
- Reuse existing CLI weights (ddl_rate, busy_rate, panic_rate) and map to generator inputs. **Implemented** via `simulator/src/args.rs`.
  - Mapping notes:
    - `ddl_rate` controls DDL frequency; `busy_rate` injects sleep while holding a connection to simulate contention.
    - `panic_rate` biases rollback vs commit in transactions to simulate failure-heavy mixes.

### Phase 3: Backend Adapters (4-8 days)
- Implement a backend trait and adapters. **Implemented** in `simulator/src/backends/mod.rs` plus adapters:
  - `backends/sqlite.rs` (rusqlite)
  - `backends/postgres.rs` (tokio-postgres)
  - `backends/mssql.rs` (tiberius)
  - `backends/turso.rs` (turso)
- Each adapter:
  - Encapsulates connection pool + checkout behavior through sql-middleware APIs.
  - Normalizes errors into a simulator error enum.
  - Exposes `execute` and `query` with consistent output shape.
  - SQLite adapter uses explicit transaction APIs and retries busy/locked errors on BEGIN/COMMIT/ROLLBACK/execute/query.
  - Postgres/Turso adapters are implemented with CLI-supplied connection options and share the same runner interface.
 - Choices/tradeoffs:
   - Backends are feature-gated; builds must enable the desired backend feature flags.
   - Postgres configuration is currently CLI-only and requires flags for all connection fields (no env var fallback).
   - SQLite generation is conservative to avoid lock errors in a single-threaded runner (fewer interleavings).

### Phase 4: Differential + Doublecheck (2-4 days)
- `--doublecheck`: run plan twice against the same backend and compare results. **Implemented** via `--doublecheck`.
- `--differential`: run plan against two backends and compare outcomes (e.g., sqlite vs turso). **Implemented** via `--differential-backend`.
- Add a `ResultComparator` to normalize result ordering and data types. **Implemented** in `simulator/src/comparator.rs`.
  - Comparisons include error class matching; error message matching is only enforced for `--doublecheck`.
  - Query row/column counts and normalized values are compared only when a reset is configured (see below).
  - Reset flags:
    - `--reset-table <table>` (repeatable)
    - `--reset-mode <truncate|delete|recreate>`

### Phase 5: Shrinking + Bug Base (3-6 days)
- Add `shrinker/` for plan delta debugging:
  - remove blocks of interactions and replay until failure persists. **Implemented** in `simulator/src/shrinker/mod.rs`.
  - optional heuristic shrink (similar to Turso).
- Add `bugbase/` with plan storage, seed, config, and last failure metadata. **Implemented** in `simulator/src/bugbase/mod.rs`.
  - CLI flags:
    - `--shrink-on-failure`
    - `--shrink-max-rounds <n>`
    - `--bugbase-dir <path>`
  - Outputs:
    - `plan.json`, `config.json`, `failure.json`, and optional `shrunk_plan.json`.

### Phase 6: Profiles + Schema (2-3 days)
- Add `profiles/` with default presets:
  - `quick`, `stress`, `ddl-heavy`, `write-heavy`
- Optional JSON schema for profile validation.

## Suggested Module Layout
- `simulator/src/args.rs` -> extend CLI with new modes and profile selection
- `simulator/src/plan.rs` -> Plan + Interaction + Assertion types
- `simulator/src/properties/` -> properties + generators
- `simulator/src/generation/` -> plan generator and workload distribution
- `simulator/src/runner/` -> execution + scheduling + trace
- `simulator/src/backends/` -> per-backend adapters
- `simulator/src/shrinker/` -> plan shrinking and replay
- `simulator/src/bugbase/` -> store/reload failures
- `simulator/src/oracle.rs` -> expanded assertions + invariants

## Immediate Next Milestone (Practical First Step)
Focus on a minimal vertical slice that proves the new design:
1) Plan type + runner that can execute against a single backend (SQLite). **Implemented** in `simulator/src/plan.rs`, `simulator/src/runner/mod.rs`, and `simulator/src/backends/sqlite.rs`, with a sample plan at `simulator/plans/first.json`.
   - Run example: `cargo run -p simulator -- --plan simulator/plans/first.json`
   - Choices/tradeoffs:
     - Uses a real `sql-middleware` SQLite pool (`bb8` + `SqliteManager`) instead of the legacy model to validate actual middleware behavior, but introduces async runtime requirements.
     - Plan execution is single-threaded and sequential per interaction; no concurrent scheduling yet, so it does not exercise interleavings until a scheduler layer is added.
     - The runner uses a minimal action set (`checkout`, `return`, `begin`, `commit`, `rollback`, `execute`, `query`, `sleep`) and logs row/column counts only; assertions and richer result comparison are deferred.
     - SQLite backend uses transaction-aware execution to avoid implicit nested transactions.
2) Two properties: `PoolCheckoutReturn` and `TxCommitVisible`. **Implemented** via `--property` with plan builders in `simulator/src/properties/mod.rs`.
   - Examples (see CLI flag in `simulator/src/args.rs`):
     - `cargo run -p simulator -- --property tx-commit-visible`
     - `cargo run -p simulator -- --property pool-checkout-return`
   - Choices/tradeoffs:
     - Properties are implemented as fixed plans (not generated) so we can assert behavior without building the generator yet.
     - Assertions are limited to row/column counts (`QueryExpectation`) rather than full result equality, keeping result normalization out of scope for now.
     - Properties run through the plan runner path (SQLite only today), so they validate middleware integration but do not yet cover multi-backend or differential modes.
   - Follow-up: once plan generation lands, properties can be expressed as invariants over generated plans; once differential/doublecheck modes land, the same properties can run across multiple backends and compare normalized results/errors.
3) JSON plan dump on failure (for both `--plan` input and `--property`-generated plans), plus a clear replay path. **Implemented** via `--dump-plan-on-failure`.
   - Examples:
     - `cargo run -p simulator -- --plan simulator/plans/first.json --dump-plan-on-failure /tmp/failed-plan.json`
     - `cargo run -p simulator -- --property tx-commit-visible --dump-plan-on-failure /tmp/failed-plan.json`
   - Choices/tradeoffs:
     - Dump path is user-specified (no auto-generated filenames) to keep behavior explicit and avoid writing unexpected files.
     - Dumping happens only on failure, and the same plan is replayable via `--plan`.
4) CLI option to control plan dump location/name and a reminder to replay the dumped plan. **Implemented** as `--dump-plan-on-failure` with a replay hint printed on failure.
5) Backend trait + adapters for SQLite/Postgres/Turso with CLI backend selection. **Implemented** in `simulator/src/backends/mod.rs` and adapters, feature-gated by backend.

## Implemented CLI Backend Flags
- `--backend sqlite|postgres|turso` selects the runtime backend (feature-gated).
- Postgres flags: `--pg-dbname`, `--pg-host`, `--pg-port`, `--pg-user`, `--pg-password` (required for Postgres).
- Turso flag: `--turso-db-path` (defaults to `/tmp/sql-middleware-simulator-turso.db`).

## Risks and Mitigations
- Backend behavior differences: normalize errors and results with a common schema.
- Flaky behavior: enforce deterministic seeds and stable scheduling.
- Complexity creep: keep the first set of properties small and expand iteratively.

## References (Conceptual)
- Turso simulator design: interaction plans + properties + deterministic runs.
- TigerBeetle DST inspiration (deterministic simulation testing model).
