# Placeholder Translation (Design Sketch)

Goal: optional, backend-aware translation between PostgreSQL-style placeholders (`$1`, `$2`, …) and SQLite-style placeholders (`?1`, `?2`, …) to reduce duplication in cross-backend query code. Default remains “no translation”.

## When Translation Runs
- Applies only to parameterized calls (`execute_select`, `execute_dml`); `execute_batch` stays untouched.
- Runs only if:
  - A translation flag is enabled (pool default or per-call override), **and**
  - The caller passed at least one parameter (empty params -> no rewrite).
- Direction:
  - Postgres target: `?N` → `$N`.
  - SQLite-compatible target (SQLite/LibSQL/Turso): `$N` → `?N`.
  - MSSQL/Tiberius: unsupported (no translation).
- Multi-statement strings are still discouraged in `execute_select`/`execute_dml`; translator assumes a single statement, as today.

## Parsing / Safety
- Implement a small lexer/state machine over the SQL string:
  - States: normal, single-quote string, double-quote identifier, dollar-quoted string (with tag), line comment (`-- …`), block comment (`/* … */`).
  - Translate **only** in normal state.
  - Placeholder detection:
    - For Postgres target: `?` followed by 1+ digits.
    - For SQLite target: `$` followed by 1+ digits.
  - Do **not** renumber; just swap the sigil.
  - Do **not** touch bare `?` or `$` without digits.
- Skip translation inside strings/comments/dollar-quoted blocks to avoid corrupting literals (URLs, regexes, JSON operators, PL/pgSQL bodies).

## API Surface
- Pool-level opt-in: add a flag to pool config/constructors (e.g., `translate_placeholders: bool`, default `false`) stored in `ConfigAndPool` so every connection knows the default behavior.
- Per-call override: include an options type (e.g., `QueryOptions`) with `translation: TranslationMode` where `TranslationMode = PoolDefault | ForceOn | ForceOff`. The fluent query builder can expose `.translation(...)` / `.options(...)` to override the pool default per call.
- Manual prepare path: expose a public helper `translate_placeholders(sql: &str, target: PlaceholderStyle, enabled: bool) -> Cow<'_, str>` (or similar) re-exported from the prelude. Manual users can translate once before `prepare` and reuse the result.

## Wiring
- Middleware paths (`select`/`dml` on the query builder): just before prepare/execute, run translation if params are non-empty and translation is enabled (resolved via per-call override or pool default) based on the active `MiddlewarePoolConnection` variant.
- Store/propagate the pool default flag so connections can access it. Likely adjustments in `ConfigAndPool` and `MiddlewarePool::get_connection`.

## Tests (to add when implementing)
- Postgres target: given SQLite-style `?1`, translation to `$1` works and executes with params.
- SQLite/LibSQL/Turso target: given Postgres-style `$1`, translation to `?1` works and executes.
- Safety: literals with `?1` inside strings are not rewritten; Postgres `?` operators and dollar-quoted blocks remain untouched.
- Per-call overrides: pool default on, per-call `ForceOff` leaves SQL untouched; pool default off, `ForceOn` translates.

## Files to Modify / Add (expected)
- New: `src/translation.rs` (translator + enums), unit tests.
- Update: pool config/constructors (`src/pool/...`, `ConfigAndPool`), executor paths (`src/executor.rs` and backend executors) to accept options/resolve translation.
- Update: public exports (`src/prelude.rs`, `src/lib.rs`) to expose translator and options types.
- Update docs/README (or examples) after implementation to show how to opt in and how to use per-call overrides/manual helper.

## Note on Turso’s own handling
- Turso uses its own lexer/parser (`turso_parser` crate). Variable tokens are produced only in “normal” scanning state:
  - `lexer.rs` matches on the leading byte; it calls `eat_var` for `?/$/@/#/:` only when not inside strings/comments/identifiers (see match around the `?` arm in `lexer.rs` ~line 210 and `eat_var` ~line 790).
  - `eat_var` parses positional `?123` or named `$foo`, returning `TK_VARIABLE`. It does not see quoted text because strings/comments were consumed by other branches.
  - `parser.rs` turns `TK_VARIABLE` into `Expr::Variable` via `create_variable` (~line 170), and auto-numbers anonymous `?` in encounter order (track `last_variable_id`).
- Bottom line: Turso already uses a quote/comment-aware tokenizer; placeholders inside quoted strings/identifiers/comments/dollar-quoted text are not treated as variables.

## Note on rusqlite/SQLite handling
- rusqlite delegates SQL parsing and parameter handling directly to SQLite’s C engine via `sqlite3_prepare_v2`/`_v3` (`rusqlite/src/inner_connection.rs` uses `ffi::sqlite3_prepare_v2`/`_v3`; see `prepare` around lines ~260+). No client-side placeholder rewrites are done in Rust.
- SQLite’s parser handles placeholders (`?`, `?NNN`, `$name`, `:name`, etc.) and is quote/comment-aware, so placeholders inside string literals or comments are not bound. rusqlite just passes the SQL through to SQLite for tokenization/binding.
- In upstream SQLite (`libsqlite3-sys` vendored `sqlite3/sqlite3.c`), tokenization is handled by `sqlite3GetToken` (~line 1815xx). Host parameters are recognized via character classes:
  - `CC_VARNUM` branch → `TK_VARIABLE` for `?` followed by digits.
  - `CC_DOLLAR` / `CC_VARALPHA` branch → `TK_VARIABLE` for `$name`, `:name`, `@name`, `#name`, etc. with identifier chars, erroring on empty names.
  - String literals, quoted identifiers, comments, etc. are consumed in other branches before variable detection, so `TK_VARIABLE` is only produced in normal SQL context.
  - The `sqlite3VdbeExpandSql` helper (around lines ~94090–94220) scans SQL with `sqlite3GetToken` specifically to find host parameters (`?`, `?N`, `$A`, `@A`, `:A`) “taking care to avoid text within string literals, quoted identifier names, and comments,” confirming the tokenizer is already quote/comment-aware for parameter detection.

## Note on tokio-postgres/PostgreSQL handling
- `tokio-postgres` does not parse/translate placeholders client-side; it sends the SQL text to the server in the extended query flow.
  - `query_typed` (and `prepare`/`query`) uses `postgres_protocol::message::frontend::parse` to send a Parse message with the raw SQL (`src/query.rs`), and `frontend::bind` to send parameter values (`postgres-protocol` crate, `message/frontend.rs`).
  - The driver relies on the PostgreSQL server’s own parser to recognize `$1`, `$2`, etc. There is no placeholder rewriting in tokio-postgres.
  - PostgreSQL’s parser is quote/comment-aware for `$n` parameters; any translation we add would be layered on top of this server-side behavior.

## Portability/Type Inference Limitations (Postgres vs SQLite)
- SQLite accepts loosely typed parameters; Postgres requires parameters to match inferred or declared types. Translation does not change SQL semantics, so mismatches can still occur.
- `RowValues::Int` now downcasts to `INT2/INT4` when Postgres infers those, but will error if the value overflows. Explicit casts are still safest for portable SQL when widths matter.
- Postgres may infer `UNKNOWN` or `NUMERIC` in queries like `SELECT $1` or `WHERE $1 IS NULL`. We currently bind ints/floats/strings/JSON only to concrete types, so untyped params can fail unless the SQL casts them (e.g., `?1::text`, `?1::numeric`, `?1::jsonb`).
- JSON: SQLite treats JSON as text; Postgres distinguishes `json`/`jsonb`. Cast portable SQL to `::jsonb`/`::json` or use the JSON RowValues variant in a typed context.
- Nulls: `Null` needs a type context on Postgres (e.g., `?1::int`); untyped null parameters will error.
- Arrays/other composite types: not supported by translation or RowValues; portable SQL using them requires backend-specific handling.***
