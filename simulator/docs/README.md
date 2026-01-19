# Simulator

## What this crate is
The simulator is a deterministic plan runner for the sql-middleware stack. It executes scripted interaction plans against a real backend (SQLite today) to validate pool/transaction behavior.

## What it does
- Runs JSON plans or built-in properties (plan builders) through the SQLite backend adapter.
- Executes pool checkout/return, tx begin/commit/rollback, and SQL `execute`/`query` actions.
- Logs a deterministic sequence of steps based on plan order.
- Validates query expectations (row/column counts) when specified in the plan.

## How to run it
Run from the repository root.

Run a JSON plan:
```bash
cargo run -p simulator -- --plan simulator/plans/first.json
```

Run a built-in property:
```bash
cargo run -p simulator -- --property tx-commit-visible
```

Pool checkout/return property:
```bash
cargo run -p simulator -- --property pool-checkout-return
```

Write logs to a file:
```bash
cargo run -p simulator -- --log /tmp/sim.log
```

Dump the plan on failure for replay:
```bash
cargo run -p simulator -- --property tx-commit-visible --dump-plan-on-failure /tmp/failed-plan.json
```

## Limitations and future ideas
Limitations:
- Single-backend (SQLite) execution only; no differential/doublecheck runs yet.
- Plan execution is sequential and single-threaded; no concurrent scheduling/interleavings.
- Query assertions are limited to row/column counts; result normalization and value equality are out of scope.
- No generator/shrinker yet; plans are either fixed JSON or built-in properties.

Future enhancements:
- Add backend adapters (postgres/mssql/turso) and differential/doublecheck modes.
- Expand error normalization and assertion depth (result ordering/value checks).
- Add plan generation, shrinking, and bugbase storage for failing plans.
- Add structured traces and replay tooling for debugging.
