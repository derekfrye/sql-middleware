# Simulator

## What this crate is
The simulator is a deterministic plan runner and plan generator for the sql-middleware stack. It executes scripted or generated interaction plans against a real backend (SQLite today) to validate pool/transaction behavior.

## What it does
- Runs JSON plans, generated plans, or built-in properties (plan builders) through backend adapters.
- Executes pool checkout/return, tx begin/commit/rollback, and SQL `execute`/`query` actions.
- Logs a deterministic sequence of steps based on plan order.
- Validates query expectations (row/column counts) when specified in the plan.
- Supports doublecheck and differential modes, plus plan shrinking and bugbase storage on failure.

## How to run it
Run from the repository root.

Run a JSON plan:
```bash
cargo run -p simulator -- --plan simulator/plans/first.json
```

Generate a plan (deterministic seed):
```bash
cargo run -p simulator -- --generate --steps 500 --seed 42 --tasks 8 --pool-size 4
```

Run a built-in property:
```bash
cargo run -p simulator -- --property tx-commit-visible
```

Pool checkout/return property:
```bash
cargo run -p simulator -- --property pool-checkout-return
```

Generate a plan with a property prefix:
```bash
cargo run -p simulator -- --generate --property tx-rollback-invisible --steps 200
```

Write logs to a file:
```bash
cargo run -p simulator -- --log /tmp/sim.log
```

Dump the plan on failure for replay:
```bash
cargo run -p simulator -- --property tx-commit-visible --dump-plan-on-failure /tmp/failed-plan.json
```

## Finding failures (ideas for "good" runs)
These patterns tend to surface bugs. Adjust as needed for your backend or pool size.

Contention and churn:
```bash
cargo run -p simulator -- --generate --steps 800 --tasks 12 --pool-size 2 --busy-rate 0.2 --sleep-rate 0.2
```

Transaction edge pressure (commit vs rollback):
```bash
cargo run -p simulator -- --generate --steps 600 --tasks 8 --max-in-flight-tx 4 --panic-rate 0.2
```

DDL interleaving:
```bash
cargo run -p simulator -- --generate --steps 700 --ddl-rate 0.25 --tasks 6
```

Differential runs with deterministic resets:
```bash
cargo run -p simulator -- --plan simulator/plans/first.json --backend sqlite --differential-backend turso --reset-table sim_gen --reset-mode truncate
```

Doublecheck for nondeterminism:
```bash
cargo run -p simulator -- --generate --steps 500 --doublecheck --reset-table sim_gen --reset-mode truncate
```

Shrink and capture failures:
```bash
cargo run -p simulator -- --generate --steps 600 --tasks 8 --shrink-on-failure --shrink-max-rounds 64 --bugbase-dir /tmp/sim-bugbase
```

Seed hunting:
```bash
cargo run -p simulator -- --generate --steps 500 --tasks 8 --seed 1
```

Deliberate error paths (hand plan):
```bash
cargo run -p simulator -- --plan /path/to/your_plan.json --dump-plan-on-failure /tmp/failed-plan.json
```

## Limitations and future ideas
Limitations:
- Plan execution is sequential and single-threaded; no concurrent scheduling/interleavings.
- Query assertions are limited to row/column counts; result normalization/value equality are only compared when resets are configured.
- Bugbase entries are only written on failures and require `--bugbase-dir`.

Future enhancements:
- Add a scheduler for concurrent interleavings.
- Expand error normalization and assertion depth (full result equality).
- Add profile presets and schema validation.
- Add structured traces and replay tooling for debugging.
