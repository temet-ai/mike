# Workflow Prompt — Table Identity Migration (paste into a new session)

> This file is a **ready-to-paste prompt**. Open a fresh Claude Code session on branch
> `claude/table-dependency-migration-BVx49` and paste everything below the line. It drives a
> coordinated team of agents through a full, verified migration of every `claude_proxy.*` base table
> onto bigint identity primary keys with real foreign keys. It is grounded in the schema as of
> 2026-05-30; the team must re-verify against live SQL, not trust the seed.

---

You are the **orchestrator** for a database table-identity migration. You will not do the schema or
Rust work yourself — you decompose it, dispatch a cross-functional team of agents, **actively
coordinate them in real time**, and gate completion behind two reviews. Follow this prompt end to end.

## Objective

Every base table in the `claude_proxy.*` schema must end with a **`BIGINT GENERATED ALWAYS AS IDENTITY`
primary key** that is the single unique indicator for the table, and every relationship must be a
**real foreign key** referencing one of those bigint identities. Along the way: de-partition `requests`,
and consolidate DB access so that **every DB operation has exactly one canonical implementation in the
persistence layer** — no SQL duplicated across call sites, no SQL issued outside that layer.

**Table renames / naming alignment are explicitly OUT OF SCOPE.** A rename breaks every query, `.sqlx`
entry, MCP tool, and frontend consumer — gratuitous churn to bundle into a data-loss-sensitive migration.
Identity + real-FK + de-partition + DB-consolidation only. Renaming is a separate, deliberate decision;
do not bundle it.

### Out of scope — track separately

**#9 dropped-write hotfix.** `projects` carries a live data-loss bug this migration does **not** fix: a
`RowNotFound` snapshot skew on the projects get-or-create upsert CTE (`INSERT … ON CONFLICT (project_path)
DO NOTHING RETURNING … UNION ALL SELECT …`). Concurrent requests sharing the `(no project)` sentinel race —
a loser sees neither the empty INSERT arm nor the winner's uncommitted row → rollback → swallowed → 2xx
with no row. De-partitioning and the bigint identity do not touch this. The fix is a bounded retry on
`RowNotFound` in the projects get-or-create plus not swallowing the early-write error — **NOT** `DO UPDATE`,
which re-introduces a per-request projects-row lock. Out of scope here; track as a separate hotfix. Full
analysis: `artifacts/2026-05-30/concurrent-settle-dropped-write-rootcause.md`.

### Definition of Done (non-negotiable; per table, with captured output)

A table's task is `completed` ONLY when ALL of these hold:
1. Migration applied — table has a `BIGINT GENERATED ALWAYS AS IDENTITY` PK, used as its unique indicator.
2. Every relationship into/out of the table is a real FK referencing a bigint identity. **No pseudo-FK
   columns remain** (no `*_id` / `*_sha256` carrying a relationship without a declared `REFERENCES`).
3. No partitions exist anywhere; `requests` is a single non-partitioned table with `id` as sole PK.
4. Every DB operation the table needs has exactly one canonical implementation in the persistence layer
   (`PersistHandle` / the repository module) — single-insert, batch-insert, upsert (get-or-create), or the
   specific read shapes (by-id, by-natural-key, projections, aggregate views) as the domain requires —
   each defined once and called by every consumer. Enforced by a **grep gate: zero `sqlx::query` /
   `query!` / `query_as` occurrences outside the persistence module.** This is the existing project
   convention (SQL belongs in `PersistHandle` methods only — `updater.rs`, `handlers.rs`, `store/mod.rs`
   must issue none); the task is to **complete and enforce** it, not invent a new pattern. It exists
   because duplicated DB code has caused real incidents here: the `response_stream_chunks` INSERT once
   lived in `store/mod.rs` in two places plus four test fixtures, and the fixtures silently drifted off the
   `session_id` NOT-NULL column because the write was not centralized.
5. The change is checked for completeness against the running code (all consumers compile and behave);
   `.sqlx/` offline metadata regenerated.
6. `cargo nextest run --all` and `cargo clippy --all-targets -- -D warnings` pass — **paste the output**.
7. The architectural-reviewer agent has reviewed the change against this objective and approved it.

**The overall work is not done until the full migration has executed and these are verified for every
base table.** A plan alone is not done; a passing test without captured output is not done.

## Read first (every agent)

- `CLAUDE.md` — project overview, agent routing table, verification discipline.
- `.claude/rules/inspection-fidelity.md` — forbidden anti-patterns (this is the audit checklist).
- `.claude/rules/testing.md` — the 10 mandatory proxy tests + **DB-backed test isolation**
  (per-test ephemeral DBs via `TestDb::acquire`; **never connect to the dev DB**; no unscoped DELETEs).
- `.claude/rules/scope-and-agents.md` — coordination rules ("idle_notification = NOT done").
- `.claude/agents/db-dev.md` — db-dev owns all SQL/migrations/sqlx; rust-developer owns surrounding Rust.

## Team & coordination

| Role | Agent | Owns |
|---|---|---|
| Schema + migrations + sqlx | `db-dev` (run several in parallel) | `*.sql` migrations, query architecture, FK/index/PK design, timeout-safe migration steps |
| Rust call-sites + repository | `rust-developer` | the shared repository, domain types, `PersistError`, rewiring every consumer, blast-radius analysis |
| Frontend consumers | `frontend-developer` | any SPA/inspect surface that reads a changed column or identity |
| Architectural review | `rust-developer` (review mode) or a dedicated reviewer | reviews each completed table against this objective |

**You (orchestrator) are never idle.** Do not wait passively for `idle_notification` — that signals a
teammate's turn ended, NOT that they are done. Actively: maintain a TaskList (one task per base table),
poll workflow agents with SendMessage for status, reorder work, and intervene the moment an agent is
blocked, looping, or drifting. Parallel `db-dev` teams work **within a dependency tier**; tiers are
**serialized** so a table's identity exists before anything that will FK to it.

Write all working artifacts under `artifacts/$(date +%Y-%m-%d)/table-migration/`.

## Phases

### Phase 0 — Setup
Confirm you are on branch `claude/table-dependency-migration-BVx49`. Read the files above. Open the
TaskList. Confirm `MIKE_TEST_ADMIN_URL` points at an admin DB (never the dev DB).

### Phase 1 — Inventory & dependency graph  (`db-dev`)
Enumerate every base table and every view from `crates/claude-proxy/src/store/persistence/migrations/`.
For each base table record: current PK column(s) + type, whether it is an auto-increment identity,
declared real FKs (`REFERENCES`), and **pseudo-FK columns** (`*_id` / `*_sha256` / natural-key columns
carrying a relationship but not declared as a foreign key). Produce `DEPENDENCY_GRAPH.md` with a mermaid
diagram (referenced → referencing) and a dependency-**tier** assignment (Tier 0 = leaf/referenced first).
Verify every fact against the live SQL — the seed inventory below is a starting point, not the truth.

Seed buckets (verify, do not trust):
- **Already identity (likely compliant):** `requests` (composite `(id,created_at)`, **partitioned** —
  must de-partition), `projects` (identity already — but a live dropped-write bug lives here; see *Out of
  scope* above, this migration does NOT fix it), `agent_dispatches`, `operator_turns`, `operator_turn_chunks`,
  `request_responses`, `request_tools`, `request_deltas`, `request_text_deltas`, `lessons`.
- **Natural / UUID / composite (need identity + FK rewire):** `sessions` (`session_id TEXT`),
  `system_prompts` (`prompt_sha256`), `messages` (`message_sha256`; has spare `message_id BIGINT`),
  `tool_uses` (`tool_id TEXT`), `tool_results`, `prompt_compositions` (UUID),
  `prompt_composition_revisions` (UUID), `filter_rules` (UUID), `agents`, `agent_instances`,
  `volatile_line_patterns` (`class_id`), `request_messages` / `request_system_prompts` (composite),
  `request_diagnostics` (1:1 on `request_id`), `backfill_messages_progress` (singleton).
- **Views (document; recreate only if a new identity column changes them — never give them identity):**
  `agent_dispatches_v`, `file_touches_v`, `operator_turn_aggregates_v`, `operator_turn_text_v`,
  `project_fingerprint_v`, `session_summary_v`, `tool_results_v`, `tool_uses_v`.

### Phase 2 — Per-table code-path dossier  (team)
Assign each table to an agent. For each, map: the owning `store/persistence/*.rs` module, cached queries
in `.sqlx/` that touch it, structs in `store/models.rs`, inspect-API handlers, `proxy-mcp` tools, and
frontend consumers. Use the manifest/parity pair (`crates/claude-proxy/src/api/manifest.rs` ↔
`crates/proxy-mcp/tests/api_parity.rs`) to be sure every reachable surface is found. One dossier per table.

### Phase 3 — Anti-pattern audit  (`db-dev` + `rust-developer`)
Check each table against the `.claude/rules/inspection-fidelity.md` catalogue: JSONB-as-storage for
derived data, recompute jobs, in-memory FK matchers, `*_preview`/`*_summary`/`*_truncated` slice columns,
denormalized aggregate columns. Record a verdict + evidence per table; fold any required de-JSONB
normalization into that table's migration design.

### Phase 4 — Per-table migration + repository design  (`db-dev` + `rust-developer`)
For each table specify: target identity PK, every FK to rewire and its bigint target, the **iterative
timeout-safe steps** (below), and the canonical per-table persistence operations the consumers need
(single-insert / batch-insert / upsert / read shapes — each defined once). Assemble `MIGRATION_PLAN.md`,
dependency-tier-ordered.

> **→ OPERATOR APPROVAL GATE.** Present `DEPENDENCY_GRAPH.md`, `MIGRATION_PLAN.md`, **and the `requests`
> id-allocator fork** (the de-partitioned `requests` MUST end DB-generated, not app-allocated — the
> operator picks one): **Option A** — `GENERATED ALWAYS` + `RETURNING` at intake (cleanest end state; but
> the id is known only *after* the INSERT, so the early INSERT moves onto the request path and the
> WebSocket "new request" broadcast must wait for it — analyze the hot-path / sequencing impact);
> **Option C** — an *atomic Postgres `nextval`* at intake + `GENERATED BY DEFAULT` (id known *before* the
> INSERT, so the WS-broadcast-before-row model and today's intake ordering are preserved, with no
> `AtomicU64` race). Both retire the in-memory allocator + `OVERRIDING SYSTEM VALUE`. Pause **once** here.
> Do not apply any migration before approval.

### Phase 5 — Execute, tier by tier  (parallel `db-dev` + `rust-developer`)
Apply migrations in dependency order. Build the shared repository; rewire all call-sites; regenerate
`.sqlx`. De-partition `requests` as its own sub-sequence (create a new **unpartitioned** table → copy
rows → swap → rebuild PK on `id` alone → rebuild dependent FKs/indexes — Postgres cannot merge partitions
back into the parent in place) **before** dependents migrate. **De-partitioning is not complete until
`requests.id` is a true DB-generated identity:** the app-side `AtomicU64` allocator + `OVERRIDING SYSTEM
VALUE` write path MUST be retired (leaving them keeps the allocator-race surface and violates the
no-app-allocation rule). That is the **D4** change — present its design fork (Option A vs C above) at the
Phase-4 approval gate before executing it; do not silently leave the allocator in place. For each table,
run both reviews (completeness-vs-running-code + full test suite, then architectural review) and mark the
task `completed` only when the full Definition of Done is met with captured output.

**Expand-only per deploy; contract in a *later* deploy.** Migrations must be expand-only within any single
deploy, and cutover happens via `scripts/deploy-proxy.sh` (which runs migrations and smoke-tests on an
**alt port before cutover**). `start-proxy.sh` strands the proxy on a migration failure, so it must **not**
be the cutover path. Each table's contract step (step 5 below — dropping the old PK / columns) ships in a
*later* deploy than the expand steps that add and backfill the new identity, never the same one. If
`scripts/deploy-proxy.sh` does not yet exist, create it (run migrations → alt-port smoke test → cut over
only on success) before any Phase 5 execution.

**Timeout-safe migration discipline (every table):**
1. *(expand deploy)* Add nullable `id BIGINT GENERATED BY DEFAULT AS IDENTITY`.
2. *(expand deploy)* Backfill in bounded batches (`WHERE id IS NULL ... LIMIT N` loops) — never one giant `UPDATE`.
3. *(expand deploy)* `CREATE UNIQUE INDEX CONCURRENTLY` on `id`; add referencing FK columns nullable, backfill in batches.
4. *(expand deploy)* Add FKs `NOT VALID`, then `VALIDATE CONSTRAINT` in a separate step (avoids the strong lock).
5. *(later contract deploy)* Promote `id` to PK, switch to `GENERATED ALWAYS`, drop the old PK.
   **For content-addressed / natural-key tables** (`system_prompts.prompt_sha256`, `messages.message_sha256`,
   `tool_uses.tool_id`, and any table whose insert dedups via `ON CONFLICT (<natural_key>)`): **demote** the
   natural key from PK but **retain it as a `UNIQUE` constraint — never drop it.** It stays the dedup key and
   the `ON CONFLICT` target that makes get-or-create race-safe; only its PK / FK-target role moves to the
   bigint identity. (The bigint id is a monotonic counter — it provides row identity, not content dedup;
   dropping the sha would let identical content create duplicate rows and reintroduce a check-then-insert race.)
6. Set explicit `lock_timeout` / `statement_timeout`; if any query runs long, split it into more
   iterations. All migrations idempotent (`IF NOT EXISTS` / re-runnable guards).

### Phase 6 — Final verification
Assert: no partitions remain anywhere; every base table's identity column is its PK and the unique FK
target; **the grep gate passes — zero `sqlx::query` / `query!` / `query_as` outside the persistence
module**; `cargo nextest run --all` and `cargo clippy --all-targets -- -D warnings` are green (paste
output). Summarize per-table status. Then open a PR.

## Hard constraints
- Never connect to the dev DB; per-test ephemeral DBs only (`TestDb::acquire`); no unscoped `DELETE`.
- No new truncation / `*_preview` / `*_summary` columns (inspection-fidelity).
- **Table renames are out of scope** — identity + real-FK + de-partition + DB-consolidation only.
- **Expand-only per deploy; contract steps ship in a later deploy; cut over via `scripts/deploy-proxy.sh`,
  never `start-proxy.sh`.**
- One operator approval gate only (end of Phase 4). The coordinator stays active throughout.
- Nothing is "done" without the full migration executed and verified against the Definition of Done.
