---
thread: restore
status: parked
last_session: 2026-06-18 (session 63a69734-e3ef-48d5-9f92-dd296e7a110a) @ d5404deb
updated: 2026-09-03
---

## State of play
Where we are: STAGE 1 (relocate local :6666 -> infra live claude_proxy) DONE+verified. STAGE 2
(merge old pre-squash 36k-request archive into the new per-domain schema) VALIDATED in scratch DB
claude_proxy_mergetest: 263/263 sessions imported, all 11 tables match the archive EXACTLY
(requests 36038, messages 76494, req.messages 3.6M, deltas 1.6M, req.tools 3.5M, stream_chunks
3.75M, tools.uses 32776, tools.results 1,657,366, responses 35480, sessions 263, operator_turns
5979), 0 FK orphans. Transform proven correct; canonical fixed script is merge-batched.sql.
SCRIPT NOW CURRENT TO 28 MIGRATIONS (2026-06-20, /implement run): baseline was 25; the 3 new
migrations were applied to the script — added Section M2 (prompt_kinds.classifications, migration
20260620000001) inside import_session, set-based via map_request, closed turns only, ON CONFLICT
DO NOTHING; the other 2 (add_claude_code_agent_id, filter_rules_inject_text) are documented
no-ops. Targeted-validated on mergetest: 5747/5747 closed turns classified, completeness
invariant 0, distribution all 'operator' (confirmed genuine — activity/hook/teammate LIKE
patterns absent from pre-Jun-2 corpus). Phase-3 reviewer APPROVED. Criteria:
artifacts/2026-06-20/implement-stage2-migrations-criteria.md.
The REAL merge into live has NOT been run yet (awaiting operator go). Live claude_proxy (current
30,617 reqs) and the archive are untouched. NOTE: local :6666 has grown to 45,741 reqs (latest
2026-06-20) — the Pi live is now ~3 days stale, so a Pi-live refresh from :6666 likely precedes
the live merge.
Next: (1) On operator go, run the SAME fixed merge-batched.sql against LIVE infra claude_proxy:
set up staging FDW + migration_maps schema in live, then the per-session/chunked/single-writer
recipe (collision-safe offsets append old data past live max ids). (2) Verify live counts =
current + old. (3) Proxy repoint: edit the local bootstrap config /Users/dawiddutoit/.config/claude-proxy/.env
database_url :6666 -> infra :5440, restart proxy. (4) Drop claude_proxy_mergetest to reclaim ~30GB.
Files: docs/threads/restore/worklog.md ; artifacts/2026-06-17/merge-batched.sql (canonical fixed
migration) ; artifacts/2026-06-17/backfill-780.sql ; /Users/dawiddutoit/.config/claude-proxy/.env.
Open questions: do the live merge now or later (operator's call).
Decisions: per-session batches + within-session request chunking (150) + single-writer + work_mem
16MB + local-socket detached driver is the only recipe that survives the 15GB shared box;
operator_turn_chunks skipped (384 vs 768-dim, re-embedded downstream); run via ssh infra.local
docker exec, NOT external :5440 (port-forward wedges under load -> docker restart network-postgres-1).
Blockers: none (awaiting operator go on the live merge).
## Stage 2 progress (2026-06-17, session 63a69734)
APPROACH DECIDED (operator-steered): once-off plain SQL ETL, NOT pipeline-replay. The old
archive shape ≈ current shape (core tables map 1:1); differences are the table-identity
"fixes": surrogate-id PKs + per-domain schema split + enum-TEXT→lookup-FK normalization.
Pipeline-replay was rejected because created_at can't be injected (now() at construction) and
the write path is event-driven with no replay code — a direct table→table ETL sidesteps all of
that (created_at is a plain column copy).

SCRATCH DB BUILT on infra: `claude_proxy_mergetest` (:5440) = current per-domain structure
(58 data tables, lookups seeded, via applying the 25 repo migration files) + schema `staging`
= 30 postgres_fdw foreign tables → `claude_proxy_premerge_archive` (old data read in place, no
copy; FDW uses internal host 127.0.0.1:**5432**, not the host-mapped 5440). staging.requests=36038.

ETL in flight: db-dev authoring `artifacts/2026-06-17/merge-old-into-new.sql` — staging.*→new
tables, fresh-id remap (old req ids 14149-83817 collide with live 1-30797), session UUID
preserved, enum-text→lookup-id resolution, created_at/body straight copy, FK-dependency order,
set-based + no-sha rules. Idempotent (truncate targets + re-run). Validate counts+integrity on
mergetest, then promote to live.

KEY ID FACTS: requests.id = bigint IDENTITY (surrogate); requests.session_id = bigint FK (UUID
lives in sessions.sessions). Old request id range 14149-83817; live 1-30797 (overlap → must remap).
REAL-RUN NOTE: final target needs public._sqlx_migrations populated (scratch lacks it — built
from files, not the sqlx migrator); irrelevant for compare phase.

INCIDENT this session: local :6666 Colima went "VM running/socket dead"; `docker-up` recovered
it, data intact (32,451 reqs, no loss). Repoint to infra NOT done (XDG config still :6666).

## Stage 2 operational facts (2026-06-17/18, session 63a69734) — HARD-WON
INFRA DB IS CONTAINERIZED: container `network-postgres-1` on infra.local (192.168.68.136),
PG17, port-map 5440->5432. I HAVE SSH: `ssh infra.local` (as dawiddutoit). Run heavy SQL via
the LOCAL socket, detached, NOT the external :5440 port-forward:
  `ssh infra.local 'docker exec -d network-postgres-1 bash -c "psql -U devuser -d <db> -v ON_ERROR_STOP=1 -f /tmp/x.sql > /tmp/x.log 2>&1; echo EXIT_\$? >> /tmp/x.log"'`
  poll: `ssh infra.local 'docker exec network-postgres-1 tail -5 /tmp/x.log'`.
The external :5440 port-forward (docker-proxy) WEDGES under heavy connection churn / big commits:
TCP accepts but PG handshake hangs, instance-wide, while the server stays healthy (few conns).
FIX = `ssh infra.local 'docker restart network-postgres-1'` (NOT a crash; disk was fine).
Avoid many competing external :5440 connections during a big commit.

ETL STATUS: script `artifacts/2026-06-17/merge-old-into-new.sql` is correct (atomic BEGIN/COMMIT,
idempotent STEP 0 truncate, set-based, no-sha-index) BUT had two FDW perf bugs being fixed now
(db-dev edit-only): body inserts used inline `ROW_NUMBER() OVER(ORDER BY)` carrying bytea →
sorted 16 GB (`requests` table) → **49 GB temp spill**, 25+ min on one table; AND rebuilt map_*
by pulling the heavy staging table a 2ND time. FIX = light-map-first (build map from id+keys
projection, then body insert joins map by old_id, no sort, single pull). FDW server fetch_size
raised 100->2000 (persists on archive_srv) — big win on the millions-row bridge tables.
mergetest staging = postgres_fdw foreign schema (host 127.0.0.1:5432 internal).

CRITICAL LESSON: db-dev's first "success / all 19 tables match, COMMIT exit 0" was NON-DURABLE —
counts were read INSIDE its open transaction; the COMMIT never landed (conn died on the wedge),
atomic rollback left tables EMPTY. ALWAYS verify counts from a SEPARATE committed connection,
never trust an in-transaction count. (No data lost: mergetest is scratch; archive+live+:6666 intact.)

Old archive (claude_proxy_premerge_archive) numbers: requests 36038 (16 GB, avg request_body
540 KB), messages 76494, req.messages 3.6M, req.tools 3.5M, stream_chunks 3.75M, tool_results
1.65M, deltas 1.6M, sessions 263. created_at range 2026-05-05..06-02. 4 non-UUID "sessions"
(no-session + 3 hash-*) get synthetic UUIDs. firing_rule_ids (3 rows) skipped (uuid[] FK to
rules absent in new schema). Old req id range 14149-83817 vs live 1-30797 → must remap (done).

NEXT after optimized script verifies clean on mergetest: run the REAL merge into live infra
claude_proxy (collision-safe offset appends old data past live max id; reversible via
created_at<2026-06-09). Then the proxy repoint (XDG config :6666->infra) + restart.

## Stage 2 PIVOT to per-session batching (2026-06-18, operator-steered)
THE MONOLITHIC SINGLE-TX ETL CANNOT RUN on the 15 GB shared infra box — 3 proven failures
moving the 16 GB `requests` body table via FDW: (1) ROW_NUMBER-over-bodies sort → 49 GB temp;
(2) fetch_size 2000 → backend OOM-killed at 11.7 GB RSS (host only 15 GB, shared w/ langfuse);
(3) fetch_size 200 → CPU-saturation/stall, even pg_total_relation_size probes timed out.
Root cause = body volume + all-or-nothing tx on small box, NOT the transform logic (which is
validated + rule-clean). Also: db-dev's "success" counts were always in-transaction illusions;
every run rolled back to empty. ALWAYS verify from a separate committed connection.

FIX (operator: "do it session by session"): restructure into per-session batches with COMMIT
per session. ~137 reqs/session, ~74 MB/tx → bounded memory, no temp, incremental durable commits,
RESUMABLE. db-dev building `artifacts/2026-06-17/merge-batched.sql`: PERSISTENT maps in schema
`migration_maps` (+ `done_sessions` progress table), global prelude (projects/sessions/agents/
system_prompts + messages.messages id-batched), then a PL/pgSQL PROCEDURE (CALL — transaction
control) looping sessions NOT in done_sessions, COMMIT each. setval once at end.

RELIABLE RUN METHOD (do NOT use external :5440 — wedges): local socket detached —
`ssh infra.local 'docker exec -d network-postgres-1 bash -c "psql -U devuser -d <db> -v ON_ERROR_STOP=1 -f /tmp/x.sql > /tmp/x.log 2>&1; echo EXIT_\$? >> /tmp/x.log"'`
External :5440 port-forward wedges under load → `ssh infra.local 'docker restart network-postgres-1'`.
fetch_size on archive_srv = 200.

NEXT: validate batched run on mergetest (committed-read counts == staging, 263 done_sessions),
then run SAME resumable script against LIVE claude_proxy for the real merge, then proxy repoint.

## STAGE 2 VALIDATION COMPLETE (2026-06-18) — mergetest perfect
The old-archive → new-schema transform is FULLY VALIDATED in claude_proxy_mergetest:
263/263 sessions, all 11 tables match the archive EXACTLY (requests 36038, messages 76494,
req.messages 3,637,250, deltas 1,636,543, req.tools 3,492,714, responses 35,480, stream_chunks
3,752,436, tools.uses 32,776, tools.results 1,657,366, sessions 263, operator_turns 5,979),
0 FK orphans, ~11.5M rows. Canonical script: artifacts/2026-06-17/merge-batched.sql (FIXED).

WHAT MADE IT WORK (the hard-won recipe — reuse for the live merge):
1. PER-SESSION batches via PL/pgSQL proc `migration_maps.import_session(uuid)` + driver
   `run_sessions(p_max int)`; persistent maps in schema `migration_maps` + `done_sessions`
   progress table → resumable. Global prelude (sessions/projects/agents/system_prompts +
   messages id-batched) once.
2. DRIVER = shell loop of `psql -c "SET work_mem='16MB'; CALL run_sessions(1)"` (per-session-
   commit-on-return; the proc's internal COMMIT does NOT flush under psql -c, and unbounded
   run_sessions(NULL) accumulates one giant tx → OOM). /tmp/drain.sh in the container.
3. WITHIN-SESSION REQUEST CHUNKING (chunk_size 150) on EVERY per-session-heavy statement
   (requests insert + all child tables B-J + the correction/agent_instance/operator_turn
   UPDATEs) — without this the big sessions (max 2431 reqs) OOM the 15 GB box. Peak RSS then
   ~2 GB.
4. SINGLE WRITER ONLY — two concurrent run_sessions = crash. work_mem=16MB caps hash/sort to
   disk. fetch_size on archive_srv = 100. Run via local socket detached (docker exec -d), NOT
   external :5440 (port-forward wedges).
5. The box CRASHED ~6× during development (OOM on big sessions / concurrency); every crash was
   recoverable (committed sessions durable, driver resumable). Autovacuum cleans the crash-bloat.

BUGS the gated ramp + final verify caught (all fixed in the script): generated-column inserts;
created_at-keyed map dup-key violations (→ carry id forward); operator_turn_chunks 384-vs-768
vector skip; and the tools.results 780-row loss (Section J per-session map_tool_use INNER JOIN
dropped cross-session tool_use_refs → fixed to resolve via globally-unique wire_tool_use_id).

NEXT — THE REAL MERGE (awaiting operator go): run the SAME fixed merge-batched.sql against LIVE
claude_proxy (already holds current 30,617 reqs). Needs: set up staging FDW + migration_maps in
live, collision-safe offsets append old data past live's max ids. Same per-session/chunked/
single-writer recipe. Reversible: old data = created_at < 2026-06-09. THEN proxy repoint (XDG
config :6666 -> infra) + restart. mergetest can be dropped after (reclaim ~30 GB).

## Session log
- 2026-06-17 (4a4c9a7f-e0d3-49cd-8056-df08f202b41b) @ 56aeab67 — thread created. Investigated
  both DBs: confirmed old infra DB reachable + pre-squash (flat schema, 36,038 reqs May5–Jun2,
  raw bodies intact), current local DB post-squash (per-domain, 30,385 reqs Jun9–Jun17). Ruled
  out migrate-forward (squash broke the path). Proposed two-stage plan: relocate+archive now,
  replay-merge later. Awaiting operator go/no-go on Stage-1 preflight.
- 2026-06-17 (4a4c9a7f-e0d3-49cd-8056-df08f202b41b) @ 56aeab67 — STAGE 1 EXECUTED + verified.
  Dropped 4 authorized stale DBs (freed 126 GB; infra was at 97%/7.9 GB free, the restore
  would have filled it). Renamed old claude_proxy -> claude_proxy_premerge_archive (36k reqs
  preserved). Created fresh claude_proxy, restored :6666 via the Docker PG16 tools (PG16->PG17).
  Verified: structure identical, migrations 25=25, row counts ~2% lower (clean snapshot).
  Found active config is the local bootstrap `.env` (=:6666), not repo
  ./.env. Repoint (1-line edit + restart) + gap decision pending operator.
- 2026-06-18 (63a69734-e3ef-48d5-9f92-dd296e7a110a) @ d5404deb — Stage 2 validated: old archive fully merged into mergetest (263/263 sessions, all 11 tables byte-exact vs archive, 0 FK orphans, ~11.5M rows); script fixed; awaiting go on the real merge into live
- 2026-06-20 (25a47388-fd57-4c1e-bbb4-ac5fe2b42ab7) @ d3c44f75 — /implement run: caught merge-batched.sql up from the 25-migration baseline to 28. Only prompt_kind (20260620000001) needed a change → added Section M2 (set-based prompt_kinds.classifications via map_request) + a STEP 10 NOT-EXISTS completeness invariant; other 2 migrations documented no-ops. Targeted-validated on mergetest (5747/5747 closed turns classified, invariant 0, all-operator distribution confirmed genuine via raw LIKE counts). Phase-3 reviewer APPROVED, all criteria met. Live merge still gated on operator go; Pi live now ~3 days stale vs :6666 (45,741 reqs).
- 2026-09-03 (thread sweep) — status active -> parked: no activity since 2026-06-20; premise is stale, the capture DB moved to armitage
