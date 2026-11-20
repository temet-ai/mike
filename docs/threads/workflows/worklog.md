---
thread: workflows
status: parked
last_session: 2026-06-18 (session cc8ad6a8-c1e8-4b08-b951-473bf5f4c43b) @ 21a1486a
updated: 2026-09-03
---

## State of play
Where we are: Workflow-run UI feature code-complete and verified on the LOCAL :6666 capture DB. Migration 20260618000001 (requests.claude_code_agent_id column + turn_origins seed id=11 'workflow_subagent') applied + recorded on :6666 and the test template; migrate-check green. Classifier (extract/request.rs ~586) tags TurnOrigin::WorkflowSubagent via the system marker "running inside a workflow script" regardless of message_count (35/35 unit tests). Endpoint GET /api/sessions/{id}/workflow-runs (handlers.rs ~9838; rehydrate.rs fetch_workflow_runs/fetch_workflow_agents ~2014) live-curl-verified on a throwaway alt-port instance: wf_0bcb789f-54b -> 4 agents / 4 results, all 9 runs bracketed with distinct counts; SQL set-based, perf 4.2ms (Pass1). Frontend WorkflowRunBand.svelte inline in RequestBand (joins by parent_request_id; label run_uid ?? name ?? run #id), types/api/turn-origins updated, 4 Storybook stories pass, svelte-check clean. Two superseded restore-thread SQL scripts deleted (merge-old-into-new.sql, patch-procedure-only.sql); kept merge-batched.sql + backfill-780.sql. NOT done: proxy restart (foreground start-proxy.sh on :6666) so the live SPA serves the endpoint; Chrome live UI check; full make check; commit; integration test (#6).
Next: (1) user restarts start-proxy.sh on :6666 (Ctrl-C + re-run); (2) Chrome verify ea593008 inline band renders agents+results+transcript; (3) make check + fidelity-check + Perf trailer (note Pass2 legacy body-scan ~9.4s budget override); (4) add deferred integration test (task #6); (5) commit.
Files: /Users/dawiddutoit/.claude/plans/enumerated-hatching-kitten.md ; crates/claude-proxy/src/api/handlers.rs ; crates/claude-proxy/src/store/persistence/rehydrate.rs ; crates/claude-proxy/src/extract/request.rs ; crates/claude-proxy/src/store/persistence/migrations/20260618000001_add_claude_code_agent_id.sql ; frontend/src/lib/components/shell/WorkflowRunBand.svelte ; frontend/src/lib/components/shell/RequestBand.svelte ; frontend/src/lib/turn-origins.ts ; frontend/src/lib/types.ts
Open questions: none (DB target is :6666; infra.local cutover belongs to the restore thread and is paused per operator)
Decisions: source from proxy DB not filesystem; join key = parent_request_id; identify workflow agents by system marker (works on pre-fix data); run membership = created_at window between consecutive Workflow tool_uses; agent result = last StructuredOutput tool_use input; agent-id from x-claude-code-agent-id header (COALESCE with new column)
Blockers: live UI verification waits on the user restarting their foreground start-proxy.sh on :6666
## Session log
- 2026-06-18 (cc8ad6a8-c1e8-4b08-b951-473bf5f4c43b) @ d5404deb — thread created.
- 2026-06-18 (cc8ad6a8-c1e8-4b08-b951-473bf5f4c43b) @ 21a1486a — Built workflow-run UI feature (inline timeline band + transcript drill-down), sourced from proxy DB; verified on :6666 (endpoint curl + 35 classifier tests + 4 Storybook stories + svelte-check). Pending only the proxy restart for live SPA check.
- 2026-09-03 (thread sweep) — status active -> parked: no activity since 2026-06-18; blocked on a restart of the retired :6666 database
