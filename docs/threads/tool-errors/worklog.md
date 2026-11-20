---
thread: tool-errors
status: done
last_session: 2026-06-16 (session 96016b68-4f39-4716-a134-495b9c904471) @ a9460edb
---

## State of play
Where we are: Implementation COMPLETE, uncommitted, not yet live. The `/diagnostics/tool-errors`
page was crashing on render (Svelte `each_key_duplicate`) and showing nothing. Root cause: the
backend feed returned one row per *replayed result block*, not per *errored call* — every follow-up
request re-sends history, so a single errored result is persisted as a fresh `tools.results` row on
each carrying request. Measured: 66,016 error rows for 1,626 real errored calls (~40× inflation).
Fixed across 3 layers (db-dev / rust-developer / frontend-developer).

Next: (1) Rebuild + restart the proxy stack (`make install` && `./scripts/start-proxy.sh`) to
activate the deduped query + facets endpoint — WARNING: this session runs through the proxy (:9988),
restart interrupts live traffic; operator's call. (2) Verify the LIVE page end-to-end post-restart
(rows dedup to ~1.6k, three dropdowns populate from facets). (3) Commit — use db-dev's `Perf-Exempt`
block (below) or the G6 commit gate blocks the SQL change.

Files:
- crates/claude-proxy/src/store/persistence/tool_errors.rs — DISTINCT ON dedup, result_id, list_tool_error_facets
- crates/claude-proxy/src/api/handlers.rs — result_id on DTO, list_tool_error_facets handler + ToolErrorFacets
- crates/claude-proxy/src/api/manifest.rs — GET /api/tool-results/errors/facets route + Exempt entry
- frontend/src/routes/diagnostics/tool-errors/+page.svelte — key on result_id, 3 populated selects, time-range presets
- frontend/src/lib/api.ts, frontend/src/lib/types.ts — listToolErrorFacets, result_id, facets type
- frontend/src/routes/diagnostics/tool-errors/page.stories.svelte — updated fixtures

Open questions: none.
Decisions:
- Identity of an errored call = its tool_use (DISTINCT ON, earliest result kept); orphan results fall
  back to result id. `result_id` is the stable frontend render key.
- NO covering index on tools.results — it's the capture hot path; per-insert write cost to speed an
  infrequent human-loaded read is the trade session_summary_v rejects. Query stands on its own at ~140ms.
- Time-range filter = presets (Any time / 1h / 24h / 7d / 30d) computing `since` client-side; replaces
  the "Since (ISO 8601)" free-text field.
- Perf: read tier, --budget 300 (session_summary_v precedent). Classified Perf-Exempt, not regression —
  the old query was 40× garbage, an invalid baseline. Tripwire: if >300ms or page goes hot, materialise
  a first_error_result_id column on tools.uses, not an index on tools.results.
Blockers: none (restart + commit are operator actions, not blockers).

## Session log
- 2026-06-16 (96016b68-4f39-4716-a134-495b9c904471) @ a9460edb — thread created. Diagnosed the
  each_key_duplicate crash → backend replayed-result-block duplication (66k rows / 1.6k calls).
  Implemented dedup query + result_id + facets (db-dev), facets route + result_id DTO (rust-developer),
  populated-select filters + time-range presets + result_id key (frontend-developer). Dropped an
  initially-added hot-path index after review; re-measured standalone at ~138ms under 300ms budget,
  Perf-Exempt. All 7 files changed, uncommitted. Not yet live — needs proxy restart.
- 2026-06-26 (cb8398c9) @ 419a466a — Closed (status: done). Verified the 2026-06-16 implementation
  LANDED on main: `list_tool_error_facets` + `GET /api/tool-results/errors/facets` route + DISTINCT-ON
  dedup are all present (commit 34af8a6e); none of the 7 files are dirty. The "uncommitted, not yet
  live" Next list (rebuild/restart/commit) is demonstrably complete. No open work.
