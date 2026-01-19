Issue 6: simulator generate run fails with nested transaction error

What failed
- Command: `cargo run -p simulator -- --generate --steps 500 --seed 42 --tasks 8`
- Result: `plan failed at step 6 (task 2): cannot start a transaction within a transaction`

Best guess as to why
- The generated plan scheduled an `execute` step that begins a transaction while that task already has an active transaction, leading SQLite to reject the nested `BEGIN`.
- This could be a missing guard in the simulator plan generator or a mismatch between per-task transaction state and the emitted actions (e.g., a second `checkout/execute` without a prior `return` or commit).
