---
thread: turn-prompt-extraction
status: done
last_session: 2026-06-19 (session 8a7e7181) @ 766041c1
---

## State of play
Where we are: DONE. "EVERYTHING UNCOMMITTED" claim in previous entry was stale — work is on main. Session-overview surface shipped: `GET /api/sessions/{id}/overview`, `get_session_overview` MCP tool, stop_response/stop_reason/interrupted fields on OperatorTurnDto, TurnBracket REPLY + INTERRUPTED badge, all color-contrast fixes. Optional follow-ups remain (teammate-message unwrapping, "(no human prompt)" placeholder, nextest db-profile entry for session_overview) but are non-blocking.
Files: docs/threads/turn-prompt-extraction/worklog.md, crates/claude-proxy/src/store/persistence/operator_turns.rs (TURN_SELECT_BASE + SESSION_OVERVIEW_SELECT_BASE), crates/claude-proxy/src/store/persistence/mod.rs (close_aborted_request, §3e lifecycle), crates/claude-proxy/src/api/handlers.rs, crates/claude-proxy/src/api/manifest.rs, crates/proxy-mcp/src/tools.rs, crates/claude-proxy/tests/e2e/session_overview.rs, frontend/src/lib/components/shell/TurnBracket.svelte, frontend/src/lib/components/shell/turnReply.ts, frontend/src/lib/components/shell/SessionTimeline.svelte, artifacts/2026-06-11/turnbracket-contrast-decisions.md
Open questions: teammate-message presentation (carried); compaction-continuation labeling (carried); whether `final_assistant_text` (everything-concatenated) should ever be re-pointed at stop_response for embeddings — deliberately NOT changed (recall may want interstitial signal).
Decisions: stop response = span-based last text-bearing primary request (NOT stamp-based, NOT the everything-aggregate); read-side computation, no materialized column (snapshot_request tx stays narrow); `final_assistant_text` semantics untouched (embeddings/recall unaffected); subagents run on sonnet/haiku only (operator rule, saved to memory); expanded-default is a per-surface prop, not a component constant.
Blockers: none.

## Misclassification inventory (2026-06-10, live DB @ :6666)

206 turns total; 134 flagged suspect (prompt >2 k chars or starting with a harness tag).
Bucketed by the start request's `human_prompt` (what a backfill would yield today):

| # | Category | Turns | Live gap? | Fix |
|---|----------|-------|-----------|-----|
| 1 | **WebFetch internal call** — one-shot, single text block `\nWeb page content:\n---\n<page>…` ending in the WebFetch copyright boilerplate; `origin=primary`, `type=user_prompt`, fired on the active model (sonnet-4-6 observed), shares the real session UUID. Session `bfffb242` has 28/30 turns of this. | 48 | **YES** — no fingerprint exists | New `TurnOrigin::WebFetchSidecar` in `extract_turn_origin` (one-shot + prefix match); sidecars never open turns |
| 2 | **Legacy write-path rows** — `user_prompt` = raw `last_user_message` (starts `<system-reminder>`/`<task-notification>`…), but the start request's `human_prompt` is already clean (34) or empty (11) | 45 | NO — fixed by b54f274 | One-time backfill from `requests.human_prompt` |
| 3 | **Local-command / slash-command tags** — `<local-command-caveat>`, `<command-name>`, `<command-message>`, `<command-args>`, `<local-command-stdout>`. Turn 434 proves a genuine human prompt can sit BELOW the tag noise. Slash-command bodies (turn 188: `/review` + full skill text) — the human action is the command name + args. | 7 | **YES** — strip list covers only `system-reminder`, `user-prompt-submit-hook`, `task-notification` | Extend `extract_human_prompt`: strip caveat/stdout tags, keep residue; if `<command-name>` present, prompt = name + args |
| 4 | **Teammate messages** — `Another Claude session sent a message:\n<teammate-message …>…` | 12 | **YES** (by design?) | Genuine conversational input — keep as turn opener; decide verbatim vs unwrapped. NOTE: TurnBracket's generic display-side tag-strip currently deletes the whole inner message and shows only the prefix line — a display-fidelity bug |
| 5 | **Compaction continuations** — `This session is being continued from a previous conversation…` (8–17 k chars) | 19 | real turns, not human-typed | Optional: label kind at read time; do NOT drop the text |
| 6 | **Genuine long prompts** (≥4 k chars, operator-authored: 370, 326, 55, 316, 290) | ~5 | not misclassified | None — disproves any length heuristic |
| 7 | False positive: prompt *quoting* `<system-reminder>` in prose (turn 290) | 1 | n/a | None — proves strip logic is sound |

Also observed: empty `user_prompt` renders as a blank `<pre>` in `TurnBracket.svelte` — no "(no human prompt captured)" placeholder yet (worklog's original Option D display half is still open).

Original worklog case (file-content injection as turn opener, session 4762d9e9 #32–37): no longer reproducible — the squashed DB (2026-06-09) holds only 3 turns for that session, and the harness-injection opener now lands as an empty prompt (turn 398), the intended fallback. Zero raw code-dump prompts exist in the current dataset; that category is presently empty.

## Issue definition

### Symptom
Operator turn `#32–37` in session `4762d9e9` shows the full content of
`crates/claude-proxy/src/store/filter/mod.rs` (≈ 15 000-char Rust source) as
the "USER PROMPT" in the TurnBracket UI instead of the human-typed question.

### Root cause (hypothesis)
The operator-turn classifier picks the **first user-role message** in the
turn's `[start_request_id, end_request_id]` span as `user_prompt`.

Request #32 (raw ID 2438) in that session has exactly one message:
```json
[{ "role": "user", "content": "<entire filter/mod.rs source>" }]
```
This message was sent by the Claude Code harness to inject file context — it is
not the human's typed question. The human's question ("Implement
project-scoped filter rules…") lives in an earlier request (#30, ID 2436) that
belongs to a *different* turn.

### Why earlier request is in a different turn
Turn boundaries are set by the classifier when a new user-typed prompt arrives.
Request #30 opened a new turn. That turn ended before request #32. The
harness then sent request #32 (the file injection) as the first request of the
*next* turn (#32–37). There was no human-typed message at the top of that
second turn — the harness injection was the opening message.

### Impact
Every turn whose first request is a harness file-content injection displays
Rust/TypeScript source code (or other file dumps) as the user prompt in:
- The TurnBracket `USER PROMPT` block in the session timeline
- The `user_prompt` column surfaced by `list_operator_turns`, `recall_similar_turns`, etc.
- Any MCP consumer reading the turn DTO

### Possible fixes

**Option A — Frontend heuristic (shallow, brittle)**
In `TurnBracket.svelte`, detect when `displayPrompt` looks like code (starts
with `///`, `//!`, `//`, `/*`, or is longer than N chars without whitespace
breaks) and render a placeholder like `[file content — see request #32]`
instead. Does not fix the DB record; search/recall still returns garbage.

**Option B — Backend: skip non-conversational messages at extraction time**
In `classify_operator_turn` (or wherever `user_prompt` is set), walk the
messages of `start_request_id` and skip messages that match a heuristic for
harness injections:
- Content is a single string ≥ 2 000 chars with no XML tags
- Content matches a known harness pattern (e.g. starts with `///`, `//!`, contains `\npub(crate)`)
Brittle; heuristics will both under- and over-fire.

**Option C — Backend: structural signal via message origin**
Harness-injected messages in Claude Code always carry a `<system-reminder>`
or are preceded by a `tool_result` block. If the first user message in a
request has no XML wrapper tags and is preceded by nothing (it IS the only
message), it may be a genuine user turn. Track whether the system produced
the message via a tool call vs a human typing.
Requires deeper audit of the Claude Code message format.

**Option D — Backend: use the previous turn's prompt as a fallback**
If `start_request_id` yields a user_prompt that looks like code/file content,
fall back to the preceding turn's `user_prompt` (or leave `user_prompt` null
and let the UI display "no prompt captured").

**Recommended approach**: Option B + D combined — skip file-dump messages
(heuristic: lone user message ≥ 2 000 chars, no harness XML tags, starts with
`//` or `/*!` or `#[`), then fall back to null rather than surfacing garbage.
Display null as "(no human prompt captured — harness injection only)" in the
TurnBracket header.

## Session log
- 2026-06-11 (f753317a-7776-457f-b27f-b0a46faeb07e) @ 2e0b8f81 — session-overview shipped end-to-end: span-based stop-response selection (validated: turn 650 → req 6787 end_turn 4,706 chars vs 5,377 aggregate; turn 641 → NULL), `GET /api/sessions/{id}/overview` + `get_session_overview` MCP tool (52 tools, binary installed), 4 additive OperatorTurnDto fields, abort-stamping fix in close_aborted_request (84 orphaned aborted requests class — partial pre-interrupt text now reaches turns), TurnBracket REPLY rewired to stop_response with INTERRUPTED badge + stop-reason chip, SessionTimeline live refetch on terminal events, nested-interactive a11y fix (stretched-sibling hit area), 62 color-contrast violations fixed (ui-designer tokens, rule codified), `initialExpanded` prop (panel collapsed / timeline expanded — caught an agent flipping the global default, reverted to prop). Backend 937+3+78, storybook 609/609, svelte-check 0 errors, fidelity CLEAN. Deployed via operator's restart; live REST verified incl. interrupted turns (session bb7dad85: interrupted=true, outcome=abandoned, pre-abort text preserved). All uncommitted — shares working tree with lazy-prompt-sections WIP.
- 2026-06-10 (c6ae1b9d-1aba-4474-b915-3d9003e630b3) — operator_turn_id fix shipped: (1) §3e lifecycle now stamps `requests.operator_turn_id` at open (INSERT…RETURNING id), extend, and the abort-replacement open; (2) new `materialize_turn_aggregates` writes the compute-once-at-close cache (tool/error/weasel counts, tools/agents/files arrays, token sums, final_assistant_text) inside the close transaction at BOTH close sites (§3e and close_aborted_request) — mirrors operator_turn_aggregates_v's live arms exactly; (3) migration `20260610000005_stamp_operator_turn_id_and_materialize` backfilled history (3,090/4,510 requests stamped; all closed turns materialized — turn 442 now reports 5 calls/{Bash,Write}/9,845 out-tokens/3,059-char assistant text, matching the UI's client-side counts). This also un-breaks closed-turn `final_assistant_text` for `get_turn_for_embedding`. Gotchas hit: migration version COLLISION with the concurrent session (both created 20260610000004; theirs = capture_inspection_views; mine renamed to 0005 + poisoned template `_sqlx_migrations` row deleted); my deploy's smoke test then lost a lock-timeout race to their concurrent deploy — THEIR deploy (same working tree, includes my code) applied 0004+0005 and is the running binary (pid 51196). Live stamping verified on fresh traffic. NOTE: previously-embedded turns were embedded with empty assistant text; embeddings are OFF in-process by design — re-embedding those turns is a follow-up for when the out-of-process embed worker lands.
- 2026-06-10 (6ffbf7aa-6c84-4dcb-928e-4c253f4c3be9) — aborted-user-prompt turn gap fixed: `close_aborted_request` closed the predecessor's open turn but never ran the §3e open lifecycle (snapshot_request never settles for an aborted stream), so an errored operator prompt floated outside every turn bracket (request 2990). Fix: abort path now opens a replacement turn anchored at the aborted request (gated on the pending→error flip; duplicate abort signals are no-ops); migration 20260610000003 repaired the 7 historical orphans as closed abandoned turns. Test extended (replacement turn + idempotence). Deployed 13:29 BST; UI verified — #24·2990 brackets correctly. OPEN QUESTION parked: nothing in Rust ever stamps `requests.operator_turn_id`, so `operator_turn_aggregates_v`'s live-turn subqueries and closed-turn membership joins see no member rows — timeline brackets work because they use start/end_request_number ranges, but the aggregates view path needs an audit.
- 2026-06-10 (6ffbf7aa-6c84-4dcb-928e-4c253f4c3be9) — origin-drift hardening shipped: new `frontend/src/lib/turn-origins.ts` is the single frontend source of truth (TURN_ORIGINS const array → TurnOrigin union, SIDECAR/HOUSEKEEPING sets, label maps typed Record so a new origin without a label fails compile); 6 consumer sites refactored onto it (types.ts, request-utils.ts, sessionTimelineGrouping.ts, RequestBand.svelte, sessionDetail.svelte.ts, bodyFetcher.svelte.ts). Drift gates: `turn-origins.test.ts` parses backend migration seeds → set-equality with TURN_ORIGINS (parser verified to extract exactly 9 codes); Rust `every_turn_origin_variant_is_seeded_in_migrations` (catches the silent-no-insert capture-loss class) + `noise_origins_and_reliable_sidecar_list_agree` (stamps_capture_identity ⟺ RELIABLE_SIDECAR_ORIGINS), with an exhaustive-match `all_turn_origins()` so new variants can't skip the guards. 113 affected frontend tests + 2 Rust guards pass.
- 2026-06-10 (6ffbf7aa-6c84-4dcb-928e-4c253f4c3be9) @ fcf0191 — DEPLOYED: stopped supervised stack, deploy-proxy.sh smoke+cutover (pid 32198), SPA relaunched detached. Verified: 164 turns (48 WebFetch deleted, 51 requests reclassified), all suspect buckets zero, 432–434 repaired, live capture clean. Thread effectively done pending optional display follow-ups.
- 2026-06-10 (6ffbf7aa-6c84-4dcb-928e-4c253f4c3be9) @ fcf0191 — implemented the fixes: WebFetchSidecar origin + fingerprint, local-command tag handling in extract_human_prompt (8 new unit tests, all passing), migration 20260610000002 (origin seed + reclassify + delete 48 spurious turns + legacy backfill + command-tag repair), validated end-to-end on a scratch DB. Caught Postgres btrim space-only default during validation. Clippy clean. Pending: proxy restart to apply.
- 2026-06-10 (6ffbf7aa-6c84-4dcb-928e-4c253f4c3be9) @ b5d60c7 — full misclassification inventory against live DB. Confirmed b54f274 fixed the write path (human_prompt → user_prompt at turn-open). Identified the three remaining LIVE gaps: WebFetch internal calls opening turns (48 rows — dominant), local-command tag noise (7), teammate-message wrappers (12); plus 45 legacy rows that backfill cleanly and 19 compaction continuations needing only a display decision. Rejected length heuristics (5 genuine ≥4 k prompts). Fix plan drafted in State of play; implementation not started.
- 2026-06-09 (bba4c251-4fd9-4e64-bffa-2e7d2e5c2e76) @ 573fa32 — thread created. Discovered the issue while verifying #32–37 prompt display in session 4762d9e9. Root cause: harness file-content injection is the first user message of the turn's opening request. Three frontend visual fixes shipped in this session: session-relative request numbering (#32–37 not raw IDs), linkable req-range chip, 200 px scroll cap on tb-prompt-text.
