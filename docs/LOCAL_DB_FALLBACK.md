---
title: Local DB Fallback & Async Resync
status: active
created: 2026-06-21
updated: 2026-10-13
summary: Reference for the shipped local-DB fallback feature — capturing to a local Postgres when the remote (shared) DB is unreachable, then draining the local-only rows back into the remote DB (row-level, idempotent, id-remapped) on the next proxy start that boots against the remote. No dump/restore.
---

# Local DB Fallback & Async Resync

Companion to [`OFFLINE_DEV.md`](OFFLINE_DEV.md#running-the-proxy-offline-local-db-fallback). This
feature is **fully implemented**: a local fallback Postgres the proxy uses automatically when the
configured remote (shared capture) DB is unreachable at boot, plus a **row-level resync** that
merges the locally-captured rows back into the remote DB. The merge is not continuous — it is one
drain per proxy start, and only on a start that reaches the remote DB, so recovering the rows
captured during an outage means restarting mike once the remote is back. **No `pg_dump`/restore**
— resync is an incremental, idempotent, app-level replication worker.

In the development stack the remote target is the shared capture database: the Docker container
`armitage-pg` (`scripts/capture-db/`), reached at `127.0.0.1:5440` on the host that runs it and at that
host's LAN address on the same port from anywhere else. Both spellings live in the untracked `.env`,
never in a tracked file (see [`ARCHITECTURE.md`](ARCHITECTURE.md#2-system-map) and
[`TROUBLESHOOTING.md`](TROUBLESHOOTING.md#capture-db-on-armitage-is-unreachable)). A stopped
container looks identical, to this feature, to the machine being off: both present as
`PersistError::Connect` and trigger the same fallback path. `docker ps --filter name=armitage-pg`
on the capture host tells them apart, and `scripts/capture-db/up.sh` is the fix for the first.

What it delivers:

1. When the remote Postgres is unreachable at boot, the proxy keeps capturing to a
   local Postgres instead of running blind or failing to start.
2. When the remote DB returns, the local-only rows are merged back into it row by row, with no
   manual step and no dump — on the **next start of the proxy against the remote DB**, not
   continuously. See [Resync worker implementation reference](#resync-worker-implementation-reference).
3. The merge is **idempotent** (safe to re-run) and **collision-proof** (local surrogate ids never
   clobber remote ones — ids are remapped).
4. No regression to the hot forward path: capture stays best-effort and never blocks proxying.

## This page is about the fallback database, not your main one

A person who installed mike without the source checkout, and who only needs a database to
capture into, wants `mike db up`, which is described in
[`GETTING_STARTED.md`](GETTING_STARTED.md#1-get-a-database-up). It runs PostgreSQL 19 with
pgvector in Docker on `127.0.0.1`, keeps the generated password in the mike config file, and
leaves the schema to mike's own migrations. `scripts/local-db/up.sh`, described below, is the
developer stack's helper for the fallback database.

`scripts/local-db/up.sh` has a known fault that is not fixed there: when `sqlx` is not on the
`PATH` it applies the migration files with `psql`, which bypasses the migration record that
mike's own migrator keeps, so the first `mike serve` against that database tries to apply the
baseline migration a second time and fails. It also needs `psql` on the host even when Postgres
runs in Docker. Install `sqlx-cli` before using the script, or use `mike db up` for a database
that is not a fallback.

## Foundations this relies on

- **Runtime capture degrades gracefully.** A failed capture write is logged `WARN` and skipped; the
  proxy still forwards the response byte-identically (`crates/mike/src/store/`). Only **boot** is
  fail-fast: if `persist_enabled` and the DB is unreachable (and no fallback applies), `AppState::new`
  aborts with `PersistError::Connect`.
- **Migrations are embedded + idempotent** and run on connect; the fallback DB needs the same three
  extensions as the primary: `vector`, `pgcrypto`, `pg_trgm` (baseline migration lines 3–5).
- **The schema is sync-friendly.** Every table uses a `bigint GENERATED ALWAYS AS IDENTITY`
  surrogate PK (so ids are local-instance-scoped and must be remapped), but nearly every entity
  table also carries a stable natural key to upsert on, and all lookup/vocab tables are seeded with
  identical fixed ids on both DBs.

## Architecture

Two pieces, both shipped.

### A. Boot-time failover (capture never goes blind)

`build_persist_handle` (`crates/mike/src/config/persist_bootstrap.rs`) probes the configured
remote URL at startup. If `ProxyConfig::database_url` fails specifically with a connectivity error
(`PersistError::Connect`), AND `db_fallback_enabled` is set, AND `local_database_url` is
configured, the proxy connects to the local fallback DB instead — capture never goes blind. A
sub-2s TCP probe (`config::primary_tcp_unreachable`) short-circuits the common "infra is down"
case instead of paying the full ~10s pool-connect timeout on every boot. The resulting handle is
tagged `ActiveDbTarget::LocalFallback` (`crates/mike/src/store/persistence/`). Any other
error from the primary (missing URL, migration failure, schema preflight) is propagated verbatim —
failover is connectivity-only; genuine misconfiguration still fails fast.

### B. Async resync worker (local-only rows → remote)

`crates/mike/src/resync/` (`worker.rs` plus the `engine/` module) — a background tokio task that,
when **both** DBs are reachable, incrementally streams rows that exist only in the local DB into
the remote DB. Never a bulk copy — it walks new rows past a persisted cursor and upserts them one
dependency level at a time.

```
 proxy capture ──► LOCAL pg ──(sync worker)──► REMOTE pg
                     ▲                              ▲
              always reachable          drained at the next boot on remote
```

Four mechanisms, all implemented:

1. **Cursor / high-water mark.** `sync.cursor(table_name, last_local_id, updated_at)` in the
   *local* DB (migration `20260621000001_sync_bookkeeping.sql`; see
   [`SCHEMA.md`](SCHEMA.md#domain-sync)). The worker only reads rows past the cursor —
   incremental and resumable across restarts.
2. **One drain per boot, no poll loop at all.** `resync::worker::spawn_resync_worker`
   (`crates/mike/src/resync/worker.rs`) runs the drain exactly once, at spawn, and the task then
   ends — there is no tick interval, no timer and no `TICK_INTERVAL` constant. **Merging
   local-only rows back into the remote DB therefore requires restarting the proxy** once the
   remote is reachable; a remote that comes back while mike is running does not trigger a drain.
   The worker also skips its drain entirely when this boot itself failed over to local (it would
   be draining and then resetting the very DB it is writing through), so the restart that drains
   is specifically a restart that boots on the remote DB. Deliberately **no** NOTIFY trigger on
   captured/synced tables either — a trigger firing per captured request would add write cost on
   the proxy hot path (the hot tier tolerates no added write cost).
3. **Dependency-ordered, idempotent replay.** `ResyncEngine::replay_pending`
   (`crates/mike/src/resync/engine/replay.rs`) walks tables in the baseline's topological order
   (`projects → sessions → messages/system_prompts → requests → responses/tools → agents →
   overrides → memory`). Each row is upserted on its natural key where one exists, never on
   surrogate id.
4. **ID remap.** `sync.id_map(table_name, local_id, remote_id)` — a durable per-row mapping.
   Inserting a parent captures `RETURNING id`, records `local→remote`, and resolves every child FK
   through the map before inserting the child. Lookup-table FKs need no remap (identical ids on
   both sides).

### Per-table replay strategy

| Group | Tables | Strategy |
|---|---|---|
| **Lookup / vocab** | `*.roles`, `*.statuses`, `*.kinds`, `*.actions`, `*.sources`, `*.methods`, `*.turn_types`, `*.turn_origins`, `*.stop_reasons`, `*.modes`, `system_prompts.blocks`, `responses.statuses`, `memory.*_outcomes/roles/kinds`, `session_judges.outcomes`, … | **Never synced.** Seeded by migrations identically on both DBs; FKs to them need no remap. |
| **Content-addressed** | `messages.messages` (`message_sha256`), `system_prompts.system_prompts` (`prompt_sha256`) | Insert-always (see below), NOT resolved by sha. |
| **Natural-key entities** | `sessions.sessions` (`session_uuid` UNIQUE), `tools.uses` (`wire_tool_use_id` UNIQUE), `projects.projects` (`project_path`), `rules.rules`/`agents.agents`/`overrides.overrides`/`overrides.revisions`/`rules.volatile_line_patterns` (`uuid`) | Resolve-or-insert on the natural key → map. Collision-proof across instances. |
| **Surrogate-only children** | `requests.requests`, `requests.{messages,deltas,diagnostics,tools,errors,system_prompts,system_prompt_firing_rules}`, `responses.{responses,errors,stream_chunks}`, `tools.results`, `agents.{dispatches,instances}`, `memory.*`, `session_judges.verdicts` | No global natural key, but genuinely new to remote (only the local instance captured them while offline). Insert + map; resolve all parent FKs through the id_map. |

### The cycle break (two-phase)

The schema has one real FK cycle — `requests ↔ agents.instances ↔ agents.dispatches ↔ tools.uses
↔ requests` — plus forward/self edges declared as plain nullable columns:
`requests.requests.{agent_instance_id, operator_turn_id, correction_request_id}`,
`requests.system_prompts.override_id`, `agents.instances.first_request_id`. The worker mirrors how
capture itself writes them: insert with those nullable FKs NULL, then a second pass `UPDATE`s them
once both endpoints are in the id_map — the same deferred-edge handling the baseline documents (see
[`SCHEMA.md`](SCHEMA.md#fk-cycle-and-trailing-alter-table-block)).

## Configuration

Boot keys, layered `defaults < selected TOML config < MIKE_* env < CLI flags` per
`crates/mike/src/config/load/mod.rs`. The fields are declared in `crates/mike/src/config/schema.rs`,
their defaults in `crates/mike/src/config/proxy_config_defaults.rs`, and the flags in
`crates/mike/src/config/load/cli_overrides.rs`:

| Key | Env | CLI flag | Default |
|---|---|---|---|
| `local_database_url` | `MIKE_LOCAL_DATABASE_URL` | `--local-database-url <url>` | unset |
| `db_fallback_enabled` | `MIKE_DB_FALLBACK_ENABLED` | `--db-fallback-enabled <bool>` | `true` |
| `db_resync_enabled` | `MIKE_DB_RESYNC_ENABLED` | `--db-resync-enabled <bool>` | `true` |

`db_fallback_enabled` gates boot-time failover; `db_resync_enabled` gates spawning the resync
worker at all. On top of the boot flag there is also a `db_resync` runtime feature toggle
(`config.feature_toggles`, settable from `/settings`; see
`20260621000002_feature_toggle_db_resync.sql`) — the worker checks the boot config **and** the
toggle, and skips the drain if either is off.

Both boot flags default on, and that is the right default on its own merits: the fallback is local-only. It neither rewrites traffic nor sends anything off the
machine — it writes captures to a Postgres on `127.0.0.1` and, later, copies those rows into the
capture DB the operator already configured. Nothing new leaves the host, so the
default-off rule for traffic mutation and content egress does not apply to it. The honest nuance is that a fallback cannot engage without a
`local_database_url` configured, so "enabled" is conditional on that URL existing — a
configuration precondition, not a default-off feature gate.

## Local Postgres provisioning

The installed provider owns local provisioning: `mike --config PRIVATE_CONFIG database
provision --port 6666` uses a PostgreSQL 19 Compose resource with an embedded image build recipe and a private
random credential. The thin development wrapper is `scripts/local-db/up.sh --config
PRIVATE_CONFIG`. There is no fixed shared password, raw SQL replay or automatic native
`/tmp` fallback. Docker and Compose are checked independently. See
[`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md) for identity, ownership and readiness rules.

Provisioning creates service/storage only. Run `mike --config PRIVATE_CONFIG serve` to
apply the embedded migrations to that selected database. Stop it before configuring a
fresh fallback for capture/resync, then transfer the URL between private configuration
files deliberately; the provider does not print credentials or overwrite another
installation's primary URL. `MIKE_LOCAL_DB_PORT` is accepted by the wrapper; the product
command uses `--port`.

The migration-seed cursor initialization is now a separate, explicit development-only
step: `scripts/local-db/baseline-fresh-fallback.sh --confirmed-fresh-fallback`, with
`MIKE_LOCAL_DATABASE_URL` set to the newly migrated fallback. It requires host `psql`.
Run it only before that fallback's first capture or resync. It baselines `sync.cursor`
past the seeded `system_prompts.system_prompts`, `overrides.overrides` and
`overrides.revisions` rows using `ON CONFLICT DO NOTHING`, so an existing advanced cursor
is not rewound. Primary setup and installed provisioning never invoke this helper.

## Resync worker implementation reference

**Lifecycle — one-shot startup drain, not a perpetual tick loop.** `spawn_resync_worker(&state)` runs
the drain exactly once, at spawn. It runs when `db_resync` is enabled, both `database_url`
(remote/dest) and `local_database_url` (local/source) are set, AND this boot's active persist target
is `Remote` — a boot that itself just failed over to local skips the drain (it would be
draining-and-resetting the very DB it is currently writing through); the next restart against remote
drains it instead. Source = the local DB (where fallback capture lands); dest = the remote DB. The
run: probe source reachability first (a local DB that isn't running is the normal steady state
between outages, not an error) — unreachable exits quietly. Then probe dest reachability (the same
fast TCP probe as boot); if unreachable, record it in `ResyncStatus` and stop. If reachable, drain
local→remote past the cursor once via `ResyncEngine::replay_pending`.

**Post-drain reset.** After a drain that reports success, `ResyncEngine::has_pending` confirms no
source rows arrived past the cursor mid-drain; if clean, `resync::reset_local` drops every
application schema plus `_sqlx_migrations` on the local DB and re-runs the embedded migrator against
it, so the next outage starts from an empty, current-schema local DB — no stale rows, no stale
`sync.id_map`/`sync.cursor` entries, no migration drift between the two DBs. A failed check or reset
is logged and left for the next restart's drain; it does not retry within the same run.

**Cheap insurance in pass 2.** The deferred-FK fixup (`fixup_deferred_fks`) joins the FK's parent
table before setting it, so a target row a mid-drain dest-side delete already removed is skipped
(left NULL) instead of failing the FK constraint and aborting the whole drain.

**Idempotency model — two guards.**
- `sync.id_map(table_name, local_id) → remote_id` is the **authoritative** "already synced"
  record. Before inserting a local row, look it up; if present, reuse the mapped `remote_id` (for
  child FK resolution) and skip the insert. This makes the worker crash-safe regardless of where
  it died.
- `sync.cursor(table_name) → last_local_id` is the **efficiency** guard: batches read `WHERE id >
  cursor ORDER BY id LIMIT N` so steady-state re-runs don't rescan. The cursor only advances after
  the batch's id_map rows are committed.

**Per-table resolution rules:**

| Class | Tables | Rule |
|---|---|---|
| **Lookup/vocab** | all seeded `*.roles/statuses/kinds/...` | **Skip** — identical fixed ids on both DBs; FKs to them pass through unmapped. |
| **Resolve-or-insert on UNIQUE key (mandatory)** | `sessions.sessions` (`session_uuid`), `tools.uses` (`wire_tool_use_id`) | A blind insert would violate the UNIQUE. Resolve the remote id by the key; insert (FKs remapped) only if absent. |
| **Resolve-or-insert on low-entropy key** | `projects.projects` (`project_path`), `agents.agents` (name/uuid), `rules.rules`/`overrides.overrides` (`uuid`) | Dimension rows shared across sessions; resolve by the natural key (a few rows, low-entropy — fine to `SELECT … WHERE key=$1`, NOT a content hash) then insert-if-absent. |
| **Insert-always + remap** | `messages.messages`, `system_prompts.system_prompts` (content-addressed), and ALL fact/child tables (`requests.*`, `responses.*`, `tools.results`, `agents.dispatches/instances`, `memory.*`, `session_judges.verdicts`) | Insert with FKs remapped via id_map, capture `RETURNING id`. Idempotency from the id_map guard, not a key lookup. |

**The content-hash decision (deliberate).** `messages`/`system_prompts` are **insert-always**, NOT
resolved by `message_sha256`/`prompt_sha256`. Re-deriving a surrogate id from a content hash is
banned in this codebase, and the sha columns are intentionally unindexed —
resolving by them would force a Seq Scan or a forbidden sha index. The accepted trade-off: a synced
offline session may write duplicate body rows into remote (same sha, new id). This is bounded (only
offline-window rows) and correct. Cross-DB sha-dedup would need operator sign-off on a last-resort
batched sha-resolve — do NOT add a sha index to enable it.

**Topological order** = the baseline's schema block order (parents first):
`projects → sessions → messages/system_prompts → requests.requests →
requests.{messages,deltas,diagnostics,tools,errors,system_prompts,...} →
responses.{responses,errors,stream_chunks} → tools.{uses,results} →
agents.{agents,dispatches,instances} → overrides.{overrides,revisions} → memory.*`.

**Generated/skip columns.** Never copy `GENERATED ALWAYS` columns
(`responses.responses.{input,output,cache_*}_tokens`) — insert source columns only and let the dest
regenerate. `bytea` bodies round-trip verbatim.

**Perf tier.** `periodic` (background worker) — budget ≤500ms/batch, measured with `db-perf-check`
against seeded data; the replay is **set-based batch** upserts (ids carried through id_map), never
per-row correlated sha subqueries.

**Status surface.** `GET /api/sync/status` — handler `get_sync_status` in
`crates/mike/src/api/handlers/usage_workflows_01.rs`, registered in
`crates/mike/src/api/manifest/requests.rs` — exposes a snapshot of the worker's last run (reachability, last-run time, per-table row counts, last error);
`SyncStatusPanel.svelte` (`frontend/src/lib/components/settings/`) renders it read-only on
`/settings`.

## Known limitations & design notes

- **Shared remote semantics.** If the remote Postgres has *other* writers, upsert-on-natural-key
  (above) is correct and collision-proof. If it's single-writer, the same worker still works —
  it's just never contended.
- **Deletes / TTL sweeps.** Resync replays inserts/updates; it does not propagate local deletions.
  `sweep_at_startup` (destructive session-deletion sweep) stays OFF by default, which avoids the local fallback DB being swept out from under an
  unsynced row — this is a standing operational mitigation, not something the resync engine itself
  guards against.
- **`bytea` bodies** (`request_body`, `response_body`) round-trip fine but dominate row size. The
  replay reads a fixed `BATCH_SIZE` of 500 rows per table pass
  (`crates/mike/src/resync/engine/mod.rs`); it is not sized by body bytes, so a table of large
  bodies moves more data per batch than a table of small ones.
