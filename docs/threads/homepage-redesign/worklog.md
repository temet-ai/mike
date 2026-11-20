---
title: Homepage redesign & feature completion
thread: homepage-redesign
status: done
created: 2026-06-17
updated: 2026-06-17
last_session: 2026-06-17 (session 4b3c987a) @ e2bfabc4
summary: Navigation-first homepage replacing the memory-usage panel; live zone wiring; new cross-session tool-uses endpoint — shipped to main and verified.
---

## State of play
Where we are: The homepage redesign feature is COMPLETE and merged to `main` (`e2bfabc4`). The old memory-usage homepage (SystemLivePanel) is gone; the homepage is now a navigation-first landing — a `DestinationNav` side-nav (Home / Sessions / Levers / Insight / System) plus bespoke zone cards with distinct form factors. Homepage shows ACTIVE sessions only (no models, UPPERCASE project caption, no "View all N" count — operator decisions). App-wide scroll-lock removed (natural document scroll). New backend `GET /api/tool-uses` (+`/facets`) cross-session feed (set-based SQL, EXISTS count, covering index migration `20260616000005`, perf-verified ≤50ms read budget) drives a new `/tools` route + `ToolsZone`. A 26-gap audit→fix pass closed every stub/blocker: `SteeringFired` WS event expanded to the 10-field contract the frontend renders, `/tools` reads `?tool_name`/`?project`, `/sessions` surfaces fetch errors, MemoryZone/SteeringZone/AnalysisZone wired to live data, all zone stories real. Verified by isolated alt-port live E2E against the real `:6666` DB (endpoint returns 30k+ rows full-fidelity, browser filter confirmed) + svelte-check/clippy gates.

Next: (1) DEPLOY to the running shared stack — operator must `git pull` main in the main checkout + restart the proxy; not done in-session because `:9988` proxies this very session (restart would interrupt it). (2) Separate test-health task for 48 pre-existing/environmental suite failures (schema/ON-CONFLICT drift + contention; unrelated to this feature) — route the schema ones to db-dev, but per the recorded lesson do NOT "restore" intentionally-squashed schema. (3) Optional polish: `trigger_request_id` present on the Rust `SteeringFired` wire but absent from the TS union (harmless); stale inspection-fidelity comment lingering in `liveSteering.svelte.ts`.

Files: docs/threads/homepage-redesign/worklog.md (this); frontend/src/routes/+page.svelte, routes/tools/+page.svelte, routes/sessions/+page.svelte; frontend/src/lib/components/home/*Zone.svelte + shell/DestinationNav.svelte; crates/claude-proxy/src/api/handlers.rs (tool-uses + list_steerers), store/persistence/tool_errors.rs (list_tool_uses + EXISTS count), steerer/fleet.rs + store/models.rs (SteeringFired); crates/claude-proxy/src/store/persistence/migrations/20260616000005_tool_uses_created_at_index.sql.

Open questions: Chase the 48 residual suite failures now, or defer to a dedicated test-health pass?

Decisions: homepage = active sessions only, no toggles (Settings owns toggles), no models, no "View all N" count; app-wide scroll-lock removed; new tool-uses endpoint is set-based (EXISTS count + covering index, no sha index); direct fast-forward push to main after gates (no PR); foreman keeps the final push as the one human-gated step.

Blockers: none for the feature. The local `:6666` `claude_proxy_test_template` was polluted (pre-squash migration state) — fixed this session by dropping + rebuilding it; dev `claude_proxy` and the `:5440` remote untouched.

## Session log
- 2026-06-17 (4b3c987a) @ e2bfabc4 — Merged the homepage prototype to main; ran audit→fix→gate→live-E2E workflows to finish the feature end-to-end (26 gaps fixed across 5 parallel units, all reviewed). Built `GET /api/tool-uses`(+facets) with covering index `20260616000005` and an EXISTS-based count (perf-verified); shipped via `89f039f1` then `e2bfabc4`, fast-forwarded onto main. Redid the full test suite: diagnosed the "284/402 failures" as stale `:6666` test-template pollution (orphaned `20260419000001` + checksum drift on `20260616000005`), dropped + rebuilt the template → 1525/1573 pass. Confirmed the remaining 48 failures are pre-existing/environmental in unrelated subsystems (websocket, e2e_smoke, compositions, stream_chunks, embed_worker, capture write-path) — not caused by this feature; the feature's own DB tests pass on a clean template.
