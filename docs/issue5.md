# Simulator crate design

Goal: add a `simulator` bin crate in the workspace (outside `src/`) to stress sql-middleware with deterministic, reproducible scenarios. Start with SQLite; make it extensible to other backends.

## Crate shape
- New workspace member `simulator/` (bin).
- Depends on `sql-middleware` via path. Own dev-only deps: `rand`/`fastrand`, `clap` for flags, `serde` for config, `tracing`/`tracing-subscriber` for logging, maybe `proptest` strategies for workloads.
- Entry: `cargo run -p simulator -- --backend sqlite --duration 60s --seed 123 --log out.log`.

## Core components
- **Fake clock**: single-threaded executor with a controllable “now”. Timers use the fake clock so interleavings are deterministic. Provides `sleep(duration)` that advances the clock and schedules wakes in a deterministic order.
- **Backend shims**:
  - For SQLite, replace the worker handle with a fake that models states (`Idle`, `InTx`, `Busy`, `Broken`) and can inject outcomes: ok, `SQLITE_BUSY`, I/O error, panic, slow execution.
  - Pluggable fault injectors: drop requests, reorder, delay, partial success, forced busy on rollback/commit, panic in worker.
  - Surface the same API shape the middleware expects so we can slot it behind feature flags or a test-only build of the simulator.
- **Scheduler/executor**: runs tasks (simulated clients) cooperatively; keeps a queue of ready tasks and timers; uses seeded RNG for event ordering (e.g., which task runs next when multiple are ready).
- **Workload generator**: produces sequences of operations per task:
  - Pool checkout/return, begin/commit/rollback, drop-in-scope, execute_batch/select, cancellations/timeouts, schema changes.
  - Parameters: pool size, number of tasks, op mix ratios, max in-flight transactions, DDL toggle, forced busy/error rates.
  - Seeds for reproducibility; ability to dump the seed and workload plan.
- **Correctness oracle**:
  - Shadow model of connection state: whether a pooled conn is idle/in_tx/broken/checked out.
  - Invariants: no checked-out broken conns, transactions end in a terminal state, rollback/commit errors mark broken as expected, pool slot ownership rules, no double-checkout.
  - After each op, compare implementation-observed state to model; assert on mismatch and emit a minimal trace.
- **Output/logging**:
  - Stream events to stdout with optional file sink.
  - Include seed, workload config, first N steps, failure trace (last M events), and oracle state diff.
  - Exit non-zero on invariant violation.

## Configuration / modes
- Flags: `--backend sqlite|postgres|...`, `--duration`, `--iterations` (alt to duration), `--seed`, `--pool-size`, `--tasks`, `--ddl-rate`, `--busy-rate`, `--panic-rate`, `--log`, `--quick` (prebaked short config), `--stress` (long-run defaults).
- Determinism: only RNG and scheduler order depend on seed; no wall-clock use.

## Integration approach
- Keep the simulator isolated; no changes to `src/` except optional feature-gated hooks if needed to swap the SQLite worker for a shim. Prefer to keep shims entirely inside the simulator crate and use the public API.
- CI: optional job running `--quick --iterations 10_000`; longer `--stress` for local/overnight runs.

## Extensions
- Add shims for other backends (network error injection, server-side aborts).
- Add metrics (counts of broken conns, max wait time) for trend tracking.
- Add replay file support: save a failing trace/seed and replay it exactly.
