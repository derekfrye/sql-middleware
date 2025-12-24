# Simulator

## What this crate is
The simulator is a deterministic load generator for the sql-middleware connection pool model. It does not connect to a real database. Instead, it models pool checkout/return, transaction lifecycles, and error injection (busy, I/O, panic) under randomized task scheduling.

## What it does
- Exercises a simplified pool state machine with multiple concurrent tasks.
- Injects random faults to validate invariants and recovery behavior.
- Logs a deterministic sequence of steps based on a seed.
- Checks invariants after every step and exits on failure with a trace of events.

## How to run it
Run from the repository root.

Basic run (defaults to sqlite backend, random seed):
```bash
cargo run -p simulator
```

Short run (preset):
```bash
cargo run -p simulator -- --quick
```

Stress run (preset):
```bash
cargo run -p simulator -- --stress
```

Fixed seed and iterations:
```bash
cargo run -p simulator -- --seed 42 --iterations 50000
```

Tune rates and pool size:
```bash
cargo run -p simulator -- --pool-size 8 --tasks 16 --ddl-rate 0.05 --busy-rate 0.02 --panic-rate 0.001
```

Write logs to a file:
```bash
cargo run -p simulator -- --log /tmp/sim.log
```

## Limitations and future ideas
Limitations:
- Models only a simplified sqlite-like backend; no real network/database I/O. The pool state machine is representative, but it is not validating driver semantics or wire protocols. This means it can miss issues that only appear in real client/database interactions.
- Error injection is probabilistic and does not model detailed error classes. Busy/I/O/panic outcomes are coarse-grained and do not reflect nuanced retry or rollback behavior. As a result, failure handling coverage is intentionally shallow.
- Scheduling is single-threaded and does not simulate true parallelism. Task interleavings are randomized, but there is no concurrent execution or timing jitter beyond the fake clock. Races that depend on actual thread scheduling are out of scope.
- No persistence or replay tooling beyond fixed seeds. You can rerun with a seed to reproduce a sequence, but there is no structured trace replay or delta debugging. Failure triage still relies on log inspection.

Future enhancements:
- Add real backend adapters (postgres/mssql/turso) to compare model vs. actual behavior. This would let the simulator run side-by-side against real clients and highlight discrepancies in pool semantics. It would also enable regression testing for backend-specific edge cases.
- Expand error classes (timeouts, disconnects, protocol errors) and recovery policies. A richer taxonomy would allow exercising more realistic retry logic and broken-connection handling. This could uncover mismatches between assumed and observed failure modes.
- Add scenario scripting for reproducible sequences beyond random weights. A small DSL or JSON script could encode specific sequences of operations, timing, and failures. That would make targeted regression cases easier to maintain.
- Export structured traces (JSON) and replay tools for debugging. Structured logs would improve tooling and allow comparisons between runs. A replay mode could step through a recorded trace deterministically.
- Integrate with property tests to search for invariant violations automatically. The simulator could be driven by a property-based engine that shrinks failing seeds. That would make it easier to isolate minimal counterexamples.
