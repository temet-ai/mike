# Workflow Prompt — Migration Squash (paste into a fresh session)

> Open a new Claude Code session **on branch `feature/migration-squash`** and paste everything below the line.
> It makes you the orchestrator of a coordinated agent team that executes the clean-break schema squash to the
> target already specified in `docs/plans/migration-squash-spec.md`, rewires every consumer, and gates completion
> behind green tests + an architectural review. The brief-cap hook has been removed, so dispatch full, detailed briefs.

---

You are the **orchestrator** of a database **migration squash**. You do **not** write the schema or the Rust/TS yourself — you decompose the work, dispatch a cross-functional team, **coordinate them in real time** (monitor, unblock, re-scope, gate), and only declare done when the Definition of Done is met with captured output. Follow this end to end. Do not stop to re-confirm decisions that are already locked below.

## The goal (one sentence)

Collapse the 104 migrations in `crates/claude-proxy/src/store/persistence/migrations/` into **one clean schema** where every base table has a single `bigint GENERATED ALWAYS AS IDENTITY` primary key, every relationship is a real bigint foreign key, **no transitional `*_ref` columns or legacy pseudo-FK twins survive**, and the whole stack (proxy + MCP + tests) compiles and passes green against it.

## Source of truth

**`docs/plans/migration-squash-spec.md`** — the per-table target spec, annotated by the operator. Read it first and in full. Each table's **→ Target** line and **NOTES** block are binding. Where a NOTES block contradicts a → Target line, **NOTES win**. Re-verify every claim against live SQL — the spec was captured 2026-06-04 from `claude_proxy_test_template`; do not trust it blindly, but do not re-open its decisions either.

## Locked rules (settled — do NOT re-litigate; the operator has corrected these repeatedly)

1. **One identity per table:** `id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY` is the sole surrogate key.
2. **`_ref` → `_id`:** every FK is a bigint column named `<parent>_id REFERENCES claude_proxy.<parent>(id)`. The transitional `*_ref` columns **and** their legacy `text`/`uuid` pseudo-FK twins collapse into that single bigint FK. **No `*_ref` column survives. No fill-trigger survives** (the Rust write path sets the bigint FK directly).
3. **session is load-bearing:** the session **string** lives **only on `sessions`** (`sessions.session_id text UNIQUE`). On **every child** table the session reference becomes a single bigint **`session_id` FK → `sessions.id`** (today's child `session_id text` + `session_ref` collapse into it). Consumers that need the *string* join `sessions`; the REST/MCP API keeps exposing the string by adding that join in the projection layer (API contract unchanged → frontend stays insulated).
4. **Owning-table natural keys stay** as `UNIQUE`: `sessions.session_id` (text), `tool_uses.tool_id` (text), and each table's own `uuid` where present (agents, filter_rules, prompt_compositions, prompt_composition_revisions, lessons, volatile_line_patterns) — these are API-facing external ids; keep them UNIQUE, `id` is still the PK. (Q1 = keep.)
5. **`firing_rule_ids uuid[]`** on `request_system_prompts` stays an array (no join-table normalisation). (Q3 = leave.)
6. **De-partition `requests`** — the clean schema defines `requests` as a single non-partitioned table with `id` as sole PK. (Falls out of writing fresh DDL.)
7. **`backfill_messages_progress`** — drop it (vestigial one-time backfill cursor; a fresh squashed schema has nothing to backfill). Confirm no live code path depends on it before dropping; if one does, report instead of forcing.
8. **DB-consolidation grep gate:** finish enforcing the existing convention — **zero `sqlx::query` / `query!` / `query_as` outside `crates/claude-proxy/src/store/persistence/`**. Move any stray SQL (≈9 sites in `api/handlers.rs`, `store/mod.rs`, etc.) into canonical persistence methods as you rewire.

## Clean-break, not expand/contract

This is a **squash on a fresh database** — there is **no data-migration path** and no expand-only constraint. Replace the 104 migration files with the clean set; existing dev/prod data is abandoned by design (the operator accepts this). Do **not** write `ALTER`-based contract steps against the live messy schema — author the target DDL directly.

## What is already prepared (do not redo)

- Branch **`feature/migration-squash`** is checked out.
- **`artifacts/2026-06-04/current-schema.sql`** — authoritative `pg_dump --schema-only` of the current (messy) schema: 25 tables, 96 indexes, 5 views, 14 fill-triggers, 25 functions, pgvector(384), partitioned `requests`. Use it as the transform reference (column types, CHECK constraints, defaults, indexes, view bodies, FK on-delete rules).
- **`docs/plans/migration-squash-spec.md`** — the annotated per-table target.
- The brief-cap hook is **disabled** — dispatch full detailed briefs.

## Team & coordination

| Role | Agent | Owns |
|---|---|---|
| Schema + migrations + sqlx | `db-dev` (run several in parallel across table-groups) | the clean squashed `*.sql`, indexes, views, FK/PK design, `.sqlx` regeneration |
| Rust call-sites + repository | `rust-developer` (several in parallel) | every query in the 13 persistence modules, handlers, MCP tools, domain types, `PersistError`, rewiring consumers onto the bigint FKs, the grep-gate consolidation |
| Frontend consumers | `frontend-developer` | only the SPA surfaces that break **if** the API contract changes (should be near-zero if rule 3 is honoured) |
| Test rewire | `rust-developer` | the ~864 test sites + the schema-shape tests (migrations_smoke, turn_chunks, migration_checksums) updated to the new constraint names |
| Architectural review | `rust-developer` in review mode (or a dedicated reviewer) | reviews each phase against this objective before it is marked done |

### Real-time coordination protocol (the operator explicitly wants this)

- **Decompose into a shared TaskList** (TeamCreate makes one). One task per table-group for schema, one per persistence module for rewire, plus phase-gate tasks. Set `blockedBy` so rewire tasks depend on the schema task.
- **Dispatch with full briefs** (hook is off). Each brief: the exact files/tables in scope, read-only vs write, the locked rules above, the DB connection, and the stop-condition ("if X, report — do not force").
- **Actively monitor — do not assume progress.** After dispatching, check on agents: read their messages, watch the TaskList, and **intervene** the moment one is stuck, looping, or drifting (e.g. "fixing" a test by routing it back onto a legacy `uuid`/`_ref` column — that is the #1 failure mode; reject it). Use `SendMessage` to redirect; refer to teammates by name.
- **Idle ≠ done.** A teammate going idle after a message has NOT finished the task; only `TaskUpdate status=completed` means done. Wait on hard preconditions — do not let a rewire agent start before the schema task is `completed`.
- **Gate each phase** behind the architectural reviewer before unblocking the next. Don't let the whole team race ahead on an unverified schema.

## Suggested decomposition (sequence the phases; parallelise within each)

0. **Schema.** db-dev authors the clean squashed migration set (one file, or a small ordered set: extension+roots → leaf tables → indexes → views), derived from `current-schema.sql` transformed per the locked rules. Verify: applies cleanly to a **fresh** DB, and a structural check passes — **no column named `*_ref`**, every FK is bigint→`<parent>.id`, `requests` is non-partitioned, dropped tables/triggers gone. **Gate before Phase 1.**
1. **Persistence layer.** rust-developer + db-dev rewire every `sqlx` call in the 13 modules (`agents.rs`, `compositions.rs`, `filter_rules.rs`, `lessons.rs`, `messages.rs`, `operator_turns.rs`, `tool_errors.rs`, `volatile.rs`, `fingerprint.rs`, `rehydrate.rs`, `backfill.rs`, `meta.rs`, `mod.rs`) onto the clean columns; centralise the ~9 stray SQL sites (grep gate); regenerate `.sqlx`.
2. **API / MCP.** rust-developer rewires `api/handlers.rs` DTO projections and `proxy-mcp` so the session **string** is still exposed (join `sessions`), MCP tool shapes unchanged. Keep manifest↔MCP parity green.
3. **Tests.** rust-developer rewires the ~864 test sites onto bigint FKs (NOT legacy columns), and updates the schema-shape assertions (e.g. `requests_session_id_fkey1` → the new `requests_session_id_fkey`; `tool_results` FK enforcement now via the bigint FK). Investigate the `listen_notify_smoke` / `db_listener_smoke` failures as you go — distinguish real trigger/notify breakage from shared-DB contention.
4. **Frontend.** frontend-developer only if Phase 2 changed any wire shape.
5. **Verify + review.** `cargo build --all`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run --all` all green with **captured output**; grep gate passes; architectural reviewer signs off.

## Definition of Done (non-negotiable; paste captured output)

1. The clean schema applies to a fresh DB; structural check confirms: single bigint identity PK per table, every relationship a real bigint FK, **zero `*_ref` columns**, zero fill-triggers, `requests` non-partitioned, `backfill_messages_progress` gone, natural keys retained as UNIQUE per rule 4.
2. **Grep gate:** zero `sqlx::query`/`query!`/`query_as` outside `store/persistence/`.
3. `.sqlx/` regenerated; `cargo build --all` clean.
4. `cargo clippy --all-targets -- -D warnings` — **0 warnings**, output pasted.
5. `cargo nextest run --all` — **0 failures**, output pasted. (No test passes by being routed back onto a legacy/`_ref` column — that is a regression, not a fix.)
6. Architectural reviewer has approved the change against this objective.
7. `git diff --stat` reviewed; the 104 old migrations are gone, replaced by the clean set.

## DB & verification environment

- **Local shared Postgres:** container `claude-proxy-postgres`, `127.0.0.1:6666`, creds `proxy`/`proxy`, DB `claude_proxy`. (The `.env` `DATABASE_URL` points at an **unreachable remote** `192.168.68.136:5440` — ignore it.)
- **Build/compile** needs a reachable `DATABASE_URL` for sqlx macro checks, OR `SQLX_OFFLINE=true` against the committed `.sqlx` cache (valid only when no SQL changed — during this migration it WILL change, so point at a live migrated DB instead).
- **Tests:** `MIKE_TEST_ADMIN_URL=postgres://proxy:proxy@127.0.0.1:6666/postgres` — `TestDb::bootstrap` creates `claude_proxy_test_template` (the clean schema) and per-test `claude_proxy_test_<uuid>` clones. **Never connect tests to the dev DB.** Per-test ephemeral DBs only; no unscoped `DELETE`.
- To recreate the template after a schema change: drop `claude_proxy_test_template`, `CREATE DATABASE`, `DATABASE_URL=…/claude_proxy_test_template sqlx migrate run --source crates/claude-proxy/src/store/persistence/migrations`.
- **Shell footgun:** in this operator's zsh, `cp` and `ls` are aliased to `copilot` — use `/bin/cp`, `/bin/ls` in scripts. `rm`/`mv` are fine.
- Honour `.claude/rules/inspection-fidelity.md` (no truncation in any capture/projection/MCP/render surface) and `.claude/rules/testing.md` (the 10 mandatory proxy tests, DB isolation).

## Guardrails

- **Do not band-aid tests onto legacy columns.** If a query breaks, move it onto the bigint FK — never re-point it at a `uuid`/`session_id text`/`_ref` column to make red go green. This is the failure mode that stalled this work for a week.
- **Surface, don't force.** If a table's NOTES are ambiguous or a drop has a live dependency you can't cleanly resolve, report it to the operator with the specific blocker rather than guessing.
- Keep `session_id` the **string** reachable everywhere it is exposed today (via the `sessions` join) — losing it from the API is a regression, not a simplification.
