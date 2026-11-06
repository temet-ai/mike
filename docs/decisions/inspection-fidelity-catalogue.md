---
title: Inspection Fidelity — Rationale & Anti-Pattern Catalogue
status: active
created: 2026-07-01
updated: 2026-07-01
summary: Extended rationale and the full historical anti-pattern catalogue for the inspection-fidelity rule, relocated out of the always-loaded rule body.
---

Full rule: `.claude/rules/agent-behavior-and-data.md`. This
doc holds the extended "why" and the anti-pattern catalogue, relocated verbatim to keep the rule
body lean.

## Why this rule exists

This proxy exists to provide **byte-identity capture and full-fidelity inspection** of Claude API traffic. Capture, persistence, projection, MCP, REST, SPA — every layer in this codebase exists so that an operator can examine the *full* content of a request, response, system-prompt block, tool call, or SSE event.

Truncation has been re-introduced repeatedly across PRs: 200-char `human_prompt` projections, `response_preview` columns, `line-clamp` on block bodies, `slice(0, n)` in Svelte templates, `truncate_for_preview` helpers, "summary" tool fields that are pre-amputated copies of full fields. Each instance is locally reasonable; stacked, they make analysis impossible. The user has had to fight every layer of this codebase to recover content the system already had.

## Anti-pattern catalogue — do not reintroduce

| Pattern | Layer | Why it's wrong |
|---|---|---|
| `human_prompt` truncated to 200 chars in `get_request_summary` / `RequestSummary::from` / `FindingDto::from` | REST projection | Forces every consumer to fetch the full body separately even when they need the prompt |
| `response_preview` projection that slices `accumulated_text` instead of cloning it | REST projection | The field name implies a slice, the value should be the full text — projections that slice without an escape hatch are pre-amputated copies |
| `truncate_for_preview(s: &str, n: usize)` / `truncate_utf8_safe(_, MATCHED_TEXT_CAP)` helpers | Backend | Their existence justifies new call sites; delete the functions, delete the temptation |
| `LEFT(blocks->2->>'text', 200)` in the anomalies SQL projection | DB / SQL | Cuts the source-of-truth before it reaches Postgres' result set — operators see "first 200 chars" with no path back to the rest |
| `MAX_ACCUMULATED_BYTES` 10 MiB SSE-buffer cap (`bounded_push_str`, `truncate_accumulated_text`) | Store | A silent cap with no surfaced flag; turned 10+ MiB streaming responses into "the analyzer ran on a partial body" with no way to tell |
| `MAX_OUTPUT_BYTES` 8 KiB cap on `claude -p` subprocess capture (`read_capped`, `truncate_utf8`) | Compose | The full subprocess output IS the inspection artifact; an 8 KiB cap with `stdout_truncated: bool` flagged but never had a "fetch the full output" path |
| `MAX_ADVICE_CHARS` 1024-char cap on operator-supplied analysis advice | Config loader | Mutates operator-authored config behind a warning; a TOML field rendered as itself in the UI must round-trip verbatim |
| `MATCHED_TEXT_CAP = 200` on weasel-pattern `matched_text` | Analyzer | The matched span IS the diagnostic; capping it to 200 chars erases the surrounding context the operator needs to act on |
| `class="line-clamp-3"` on a system-prompt block body | Frontend | Block bodies must render in full (agent memory: `feedback_no_display_truncation`) |
| **Frontend row-content truncation** in any form: `text-overflow: ellipsis` + `nowrap`, `truncate(block.text, N)`, `blockTitle` first-line slice, `text.slice(0, N) + '…'`, "expand for more" toggles on rows that are display surfaces | Frontend / Frontend CSS | All five are the same defect: a row that hides content the operator cannot otherwise see. The row must grow; collapsed-row previews are still display surfaces. The rule applies even when a row "looks chrome-y." |
| `tool_names: Vec<String>` projected from full `tool_calls` JSONB at write time, with no path back to arguments | Schema | The inspection consumer cannot answer "what arguments did this tool get?" |
| `event_count` field with no way to retrieve the events | REST | Counters without drill-down are gaslighting |
| **JSONB-as-storage for derived data** — denormalised aggregate columns rebuilt by a periodic recompute job over a JSONB blob (e.g. pre-cutover `requests.tool_calls` + `requests.tool_results` blobs feeding `operator_turns.tool_call_count` / `tools_used` / `files_touched` via `recompute_for_session`) | Schema | The aggregate columns drift from source whenever the recompute is missed, partial, or races. The only honest place for `tool_call_count` is `SELECT COUNT(*) FROM tool_uses WHERE request_id BETWEEN <start> AND <end>`. Promoting derived data into a column is fine when it's a materialised cache fed by a trigger; pulling it from a JSONB blob on a session-keyed schedule is not. The 2026-05-05 cutover replaced this shape with normalised `tool_uses` / `tool_results` / `agent_dispatches` tables and read-side aggregate views (`operator_turn_aggregates_v`). Do not re-introduce JSONB columns or recompute jobs as a perf optimisation; if measured aggregate latency hurts, materialise via trigger into a cache table. |
| **in-memory matcher index for FK-shaped relationships** — a HashMap (e.g. pre-cutover `tool_use_id → request_id` matcher in `store/mod.rs`) used to link two persisted entities that should be FK-related on disk | Backend | The matcher only works while the proxy is running. A restart between a tool_use being captured and its tool_result arriving silently drops the linkage — there is no FK to enforce it on disk, and the on-disk JSONB shape carries the use_id without the back-pointer. The 2026-05-05 cutover replaced the matcher with `tool_results.tool_use_id REFERENCES tool_uses(tool_id)`. Do not re-introduce in-memory indexes for relationships the database can model directly — they are restart-fragile and silently violate the inspection-fidelity contract (a restored row joins, an in-memory-matched row goes missing). |
