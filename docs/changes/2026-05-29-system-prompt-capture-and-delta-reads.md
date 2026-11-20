# 2026-05-29 — System-prompt capture fidelity + delta-based reads

Status: **backend SHIPPED + verified; frontend (Fix 2b) WIP, uncommitted, not yet compiling** · Team: `body-cutover` (stood down) · Tasks: #4 ✅, #5 ✅, #6 ⏳

> **Session-close state (2026-05-29):** Fixes 1 + 2a are complete, tested, and clean.
> Fix 2b is partial — the SPA edits are in the working tree but do **not** compile yet
> (see the resume note under Fix 2b). Nothing is committed. To resume: finish Fix 2b to a
> passing `pnpm run check`; to abandon: `git checkout -- frontend/src/lib/{api.ts,types.ts,stores/bodyFetcher.svelte.ts,stores/sessionDetail.svelte.ts}` and remove the new `bodyFetcher_delta.ts`.

## Summary

An investigation into why the `messages` / `request_messages` / `request_deltas` dedup
tables were populated but not load-bearing produced one **decision** and three **fixes**:

- **Decision — keep `requests.request_body`.** It is the true byte-for-byte record and the
  safety net for any future extraction; nulling it (the "storage cutover") is **cancelled**.
  We pursue only the **retrieval** goal: read from `request_deltas` + `messages` for
  targeted analysis, with the inline body as the fallback source.
- **Fix 1 — persist the system-prompt identity on EVERY request** (capture-fidelity bug).
- **Fix 2a — surface `predecessor_request_id` on the request-summary projection.**
- **Fix 2b — SPA reads `GET /api/requests/{id}/deltas`** instead of fetching two whole
  request bodies per band and diffing in-browser.

## Context — what the investigation found

The dedup store is fully populated but the inline `request_body` (~7.2 GB, ~26k rows) was
never nulled and remained the read source. Two capture/attribution gaps surfaced:

1. **System prompt persisted only on `PrimaryFirst` turns.** Only ~204 / ~27,000 requests
   had a stored `request_system_prompts.prompt_sha256` because the composer's
   `classify_and_compose` (which produces the sha+blocks) ran only on `PrimaryFirst`
   non-probe turns; every other request took the `Passthrough { prompt_sha256: None }`
   branch. The raw `system` was extracted on every request but only used for persona
   detection, never persisted. → **Fix 1.**

2. **Apparent per-request system-prompt churn was an artifact, not real mutation.**
   Within a session each agent's prompt is stable. Two confounds inflated the raw distinct
   count from the true ~archetype count to ~1,646: (a) a volatile billing-header block
   (`cc_version=…; cc_entrypoint=…`) prepended to `system`; (b) the proxy lumping the main
   agent + utility sidecars into the `subagent_persona = NULL` bucket (persona attribution
   covers only ~2.3% of requests). The composer **already** strips the billing line via the
   volatile-line registry (`compose_prompt/volatile.rs`, Phase 6.3) before hashing, so its
   `prompt_sha256` is billing-invariant and cache-safe — the fix reuses that, it does not
   add new volatile detection.

Full investigation + the (cancelled) storage-cutover analysis are in the working notes
(local, git-ignored): `artifacts/2026-05-29/cutover-investigation/` — `CUTOVER-PLAN.md`,
`db-findings.md`, `backend-findings.md`, `frontend-findings.md`.

## Changes

### Fix 1 — per-request system-prompt capture (backend, SHIPPED)
- Added capture-only `ComposerService::observe_prompt_identity()`
  (`compose_prompt/mod.rs`), reusing `parse_system_blocks` (`blocks.rs:154`) →
  `strip_volatile_lines` (`volatile.rs`) → `prompt_sha256_from_block_text`
  (`classify.rs:20`) — byte-identical to the classifier's sha.
- Wired in `proxy/handler.rs:413`, computed **before** the compose match, applied only when
  classification produced no sha and the shape wasn't unknown. **Upstream bytes untouched**
  (capture-only); the `PrimaryFirst` rewrite gate is preserved (its rationale is cache
  warmth — handler.rs:370–381).
- Verification (captured, 0 failures): unit `observe_prompt_identity_matches_classifier_sha`
  (3/3); e2e `continuation_persists_system_prompt_identity` + regression (4/4 in
  `tests/e2e/eligibility_excludes_noise.rs`) — a non-`PrimaryFirst` continuation now writes
  a 64-hex `prompt_sha256` + a `system_prompts` row and dedups to one sha with its
  PrimaryFirst head; quota/count_tokens probes correctly still yield no sha.

### Fix 2a — predecessor_request_id on summary (backend, SHIPPED)
- `predecessor_request_id` already existed on both `RequestDetail` and `RequestSummary`
  with `#[serde(skip_serializing_if = "Option::is_none")]`; `RequestDetail` was already
  populated. The only gap was `get_request_summary` returning `None` — now populated
  (`api/handlers.rs`). **Wire contract:** integer when present, **key absent** (not `null`)
  when there is no predecessor.

### Fix 2b — SPA reads /deltas (frontend, WIP — UNCOMMITTED, NOT COMPILING)
Intended: `GET /api/requests/{id}/deltas` already exists (W2.8) returning full canonical
message JSON inline per row; the SPA never called it. The change adds `getRequestDeltas`
(`api.ts`), `RequestDeltasPayload`/`DeltaRowDto` + `predecessor_request_id?: number` on the
request types (`types.ts`), rewrites `useLazyDelta` to consume `/deltas` and render
`DeltaRowDto.body` directly, surfaces `removed`/compaction rows, keeps `walkMessageDelta` as
the empty-deltas fallback (~3% non-primary lineage). Retrieval optimization, not display
truncation — bodies still render in full. Contract: `predecessor_request_id` key is ABSENT
(not `null`) when there is no predecessor.

**Resume note (where it stopped at session close):**
- `api.ts` (+23 lines) and `types.ts` (+88 lines) — appear complete (getRequestDeltas +
  the new DTOs/fields).
- `stores/bodyFetcher.svelte.ts` (~384 lines churned) + a NEW file
  `stores/bodyFetcher_delta.ts` — the agent began extracting the delta logic into a new
  module. **This is the incomplete part:** `bodyFetcher_delta.ts` has unresolved imports
  (`MessageDelta`, `BodyPayload`, `DeltaRowDto`, `RequestDetail`, `$state`, plus local refs
  `walkMessageDelta`/`peekBody`/`loadBody`/`classifyShapeBeacon`) — it does not type-check.
- NOTE: `bodyFetcher_delta.ts` was a new file outside the original allowlist (the agent
  chose to split the module). Decide on resume whether to keep the split or inline it back.
- To finish: resolve the imports in `bodyFetcher_delta.ts`, wire it into
  `bodyFetcher.svelte.ts`, then `pnpm run check` → 0 errors + the inspection-fidelity grep,
  + a live `/deltas` UI render.

## Evidence sessions (proxy inspect)

- `6ed89307-ecac-4248-99ba-babd6b75c185` — busy opus-4-8 session (997 reqs); billing-header
  confound. <http://localhost:9990/sessions/6ed89307-ecac-4248-99ba-babd6b75c185>
- `52fc6d7a-21e4-4a44-9ff8-754b73cbb468` — 4 subagent personas; within-session per-agent
  stability proof. <http://localhost:9990/sessions/52fc6d7a-21e4-4a44-9ff8-754b73cbb468>

## Items done

- Team `body-cutover` · Tasks #4 (Fix 1), #5 (Fix 2a), #6 (Fix 2b).
- Memory: `debugging_system_prompt_capture_gap.md`, `project_request_body_dedup_decision.md`.

## Follow-ups (out of scope here, logged)

- **Persona attribution** — only ~2.3% of requests carry `subagent_persona`, ~1.3%
  `agent_instance_id`; the main agent vs sidecars aren't distinguished. Capture gap.
- **`tool_uses` FK write-as-you-go race** — `WARN tool_use DB insert failed … Phase 11
  crash-recovery will retry at terminal`: tool_use insert can beat its request-row commit;
  self-heals at terminal. Pre-existing (write-as-you-go), not introduced here.
- **`request_deltas` slim (~3.4 GB)** — it inlines a full body copy per row despite a
  `message_id` FK; could resolve via JOIN. Standalone win, deferred (B0 in CUTOVER-PLAN.md).
