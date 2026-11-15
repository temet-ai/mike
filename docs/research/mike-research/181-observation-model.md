---
title: "#181 runtime-neutral observation layer: where it stands and what to do next"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: The layer #181 asks for was mostly built on 11-12 (normalized tables for Codex, views over Claude's capture), but no Codex turn has reached it since. Finish the hybrid; don't start another design. Prove it with one live Codex turn after fixing #179's parse, then move the remaining direct readers.
session_id:
  - 1c4214e6-bb47-45a3-9c04-68256a8ad430
---

## What is true now

Measured 2026-11-15 on omit, live build `ef99f7e16` (= `main` HEAD), read-only.

**Most of the layer already exists**, from commit `c4cd3c3b1` (2026-11-12 12:08 UTC). It is a mix of options (a) and (b):

- Normalized tables: `runtime.session_facts`, `runtime.model_calls`, `runtime.session_items` and `runtime.observation_receipts` (migration `20261112130000_runtime_session_observations.sql`). One store entry point writes them: `apply_session_observation` (`crates/mike-runtime/src/store/persistence/runtime/observation.rs:20`, then `observation_write.rs:6`).
- Three adapters call it:
  - the Responses wire adapter, `crates/mike-features/src/proxy/responses_capture/wire_observation.rs:107`
  - Codex hooks and OTel, `crates/mike-runtime/src/runtime/codex_observe/ingest.rs:80,215`
  - the managed app-server, `crates/mike-launch/src/launch/codex/observation.rs:115`
- Claude's adapter is a set of views over its existing capture, `20261112130300_runtime_common_readers.sql`:
  - `runtime.model_call_facts` (line 1) is a UNION of `requests.requests` and `runtime.model_calls`. Claude rows are used only where the session has no `session_facts` row (line 21).
  - `runtime.session_activity()` (line 43).
  - `runtime.session_observation_v` (line 57) and `runtime.session_observation_summary_v` (line 89).
- A third-runtime test already exists: `crates/mike-fleet/src/launch/cross_crate_tests/runtime_observation_tests.rs:15`, `observation_third_runtime_drives_roster_fleet_and_summary`. It uses runtime `"third-fixture"` (`tests_observation_support.rs:60`). The observation's `runtime` is an open string, but `RuntimeKind` (`crates/mike-base/src/runtime/identity.rs:6`) is still the enum `{Claude, Codex}` used by launch.

**Readers that query `runtime.codex_*` or `responses_exchanges`, outside the store that writes them: 0.**

- All 28 files with `runtime.codex_*` (80 references) are in `crates/mike-runtime/src/store/persistence/runtime/`: projection writes, the codex intake and retention.
- `responses_exchanges` appears in 5 files: the insert, retention, and two test files.

The first half of the done-when is already true in the code. What is missing is **data**.

**The normalized tables have never held a row.**

| Table | Rows |
|---|---|
| `runtime.session_facts` | 0 |
| `runtime.model_calls` | 0 |
| `runtime.session_items` | 0 |
| `runtime.observation_receipts` | 0 |
| all 13 `runtime.codex_*` | 0 |
| `runtime.namespaces`, `threads`, `events` | 0 |
| `runtime.responses_exchanges` | 1,470 (metadata kept 7 days) |

Of the 917 `/codex/responses` exchanges, all were made on 2026-11-12 between 08:50 and 12:24 UTC. That window ends 16 minutes after the adapter landed and before the adapter could have been deployed. They break down as 276 `complete`, 622 `aborted` and 19 `refused`. `response_id` is set on 0 of 917, `usage` on 0 of 1,470 and `response_state` on 0. Since then the only Codex traffic has been 473 `/codex/models` polls. No Codex turn has reached the new layer, so no fix for #179 or #180 has been seen working live.

**Audit-table readers** (`docs/RUNTIME_CONTRACT.md:113-126`), by what they read today:

| Reader | Reads | Moved? |
|---|---|---|
| `crates/mike-fleet/src/store/persistence/fleet/fleet_seat_rows.sql:19-47` | `session_observation_v`, `model_call_facts` | yes |
| `crates/mike-base/src/store/persistence/workspace/roster.rs:136-142` (list), `:220-224` (single) | `session_facts` plus an inline LATERAL on `requests.requests` that mirrors the view; `session_activity()` | yes (the list copies the view's fallback instead of calling it) |
| `crates/mike-base/src/store/persistence/rehydrate/session_summary.sql:39-41` | views | partly: lines 29, 50 and 88 (naming from Claude prompts) and **146 (newest status straight from `requests.requests`)** |
| `rehydrate/session_head.sql:8`, `session_listing_tail.sql:24-31`, `launch/adopt.rs:22` | views | yes, but `session_head.sql:23,45,64` still read `requests.requests` |
| `crates/mike-fleet/src/store/persistence/fleet_correlation/nodes.rs:128,135` | `requests.requests` | no |
| `fleet/run_record_sql/seats.sql:18`, `tokens.sql:21-22`, `fleet/effort.rs:71,148-154,213` | `requests.requests`, `responses.responses`, `tools.uses` | no |
| `fleet/tree_results.sql`, `slow_commands.sql`, `background_ends.sql`, `tree_changes_since.sql`, `run_record_sql/cargo_repeats.sql` | `tools.uses` and `tools.results` (Claude tool records) | no; no normalized command or file-change fact exists yet |
| `launch/prompt_turn_seen.sql:3`, `launch/list_launches_select.sql` (brief consumption) | `requests.requests`, `tools.uses` | no |
| mailbox: `redelivery.rs`, `session_reads.rs`, `team_comms.rs`, `undelivered_inactive_due.sql` (`sent_request_id`) | `requests.requests` | no |
| `run_record_sql/forks.sql` | `launch.launches` only | not request-dependent |

**Summary.** 6 reader files moved (9 files reference the views). **18** non-test files in `mike-fleet` and `mike-launch` still read `requests.requests` directly. There are 185 such files across all crates, but most are Claude-only features (filters, lessons, dossier) that will not move. 86 files read `tools.uses`/`tools.results` and 34 use `sent_request_id`.

**Read cost of the views today.** On a 3,126-request Claude session (id 2204753), EXPLAIN ANALYZE gives:

| Query | Time |
|---|---|
| `session_observation_summary_v` | 32.2 ms (plan 3.6 ms) |
| `session_observation_v` | 0.2 ms |
| a direct `requests`⋈`responses` aggregate | 11.6 ms |

The summary view's UNION and NOT EXISTS cost about 3× the direct read, once per session row.

## Options compared

| | (a) SQL views over today's tables | (b) normalized tables, one adapter per runtime | (c) Rust reader trait, one implementation per runtime |
|---|---|---|---|
| What it means | Views over `codex_*` and `responses_exchanges` too, parsing bodies at read time | Every runtime writes `session_facts`, `model_calls` and `session_items` (Claude too, not only through a view) | `trait ObservationReader` with Claude and Codex implementations; readers call Rust instead of SQL |
| Already built | Claude half (4 views) | Codex half (3 adapters, 4 tables) | nothing |
| Work to finish the done-when readers | 1 migration; a Codex view must parse JSON out of `bytea` bodies on every read | the 2 remaining reads in `session_summary.sql`/`session_head.sql`, then `nodes.rs`, `seats.sql`, `tokens.sql`, `effort.rs`: about 6 files and 9 queries | rewrite the 18 direct readers plus the 6 moved ones (24 files) into multi-query Rust. They join `sessions`, `launch`, `mailbox` and `workspace` in one SQL statement today (for example the 56-line `fleet_seat_rows.sql`), so each becomes N+1 queries or a per-runtime copy of the same SQL |
| Write cost | 0 | Codex: one `model_calls` row per response (917 on the busiest day). Claude, if copied as well: about 8,000 rows/day (55,899 requests in 7 days), which the views make unnecessary | 0 |
| Read cost | highest: JSON parsing per read; history disappears with the 7-day `metadata_expires_at` | an indexed table read; the UNION costs 32 ms vs 11.6 ms on a large Claude session | depends on each implementation; loses SQL joins |
| #179 (usage empty, finished turns `aborted`) | not fixed: nothing parses usage or `response.completed` at capture | fixed at the adapter: `wire_observation.rs` turns the relay's `summary` into a `ModelCallObservation` with usage and status. The parse feeding it is what #179 found broken (`response_id` 0/917) | not fixed: same parse gap |
| #180 (roster calls Codex `uncaptured`) | fixed if `session_activity` reads the Codex view | fixed in the code already: `roster.rs:136` prefers `session_facts`, `:127` gives `uncaptured` only when there is no activity. It needs the Codex adapter to write a fact | fixed per implementation |
| A third runtime | another set of views per vendor shape | an adapter plus a launcher; the readers stay the same (the `third-fixture` test proves this) | another trait implementation for every reader |

## Recommendation

**Keep the hybrid that is already built and finish it: (b) for every runtime except Claude, and views for Claude.** Don't start option (a) or (c). Building (c) would mean rewriting 24 files to make the same answer slower to get. Building (a) would mean parsing bodies on every read and would still leave #179 broken. Don't copy Claude into `model_calls` either: the views already show that Claude can stay raw, and the 3× UNION cost is 20 ms on the largest sessions.

Order:

1. **Fix #179 first, in the wire adapter's input.** The relay never sets `response_id`, usage or `response_state` (0 of 917), so `wire_observation.rs` has nothing to write. Until a captured turn produces a `model_calls` row, #180's roster fix and the whole layer stay unproven.
2. **Then prove #180 with one live `cdx exec hi`.** It needs a `session_facts` row with runtime `codex`, a `model_calls` row with tokens, and a roster row that reads `live` and later `stale`/`ended`, not `uncaptured`.
3. **The first reader to move is `crates/mike-base/src/store/persistence/rehydrate/session_summary.sql:146`.** Its newest status comes straight from `requests.requests`, although the view it already joins (line 39) computes `last_status`. It is part of the done-when (session summary), it is one LATERAL, and it is the line that would show a Codex session's status as blank. `session_head.sql:23,45,64` goes with it.
4. Then `fleet_correlation/nodes.rs:128,135`, followed by the run record (`seats.sql`, `tokens.sql`, `effort.rs`) onto `model_call_facts`.
5. Only after that, add the command and file-change facts that the `tools.uses` readers need (tree results, slow commands, tree changes, brief consumption), as new `session_items` kinds. Leave the mailbox `sent_request_id` work to C04.
6. Make the roster list call `runtime.session_activity()` instead of keeping its own copy of the fallback (`roster.rs:136-142`), so one definition decides the Claude fallback.

## Measure

- **Live layer**: rows in `runtime.observation_receipts` and `runtime.session_facts` with `runtime = 'codex'` go from 0 to above 0 after one Codex turn. `count(model_calls with input_tokens) / count(complete /codex/responses exchanges)` goes from 0/917 to 1.0.
- **#180**: that session's roster row reads `live` with a `last_request_at`, and then `stale`/`ended` after exit. It must never read `uncaptured` once the 10-minute grace has passed.
- **Static ratchet**: files outside `crates/mike-runtime/src/store/persistence/runtime/` that query `runtime.codex_*` or `runtime.responses_exchanges` stay at 0 (0 today). Audit-table reader files that read `requests.*` or `tools.*` directly go from 18 to 0, the done-when three first (`session_summary.sql` and `session_head.sql`: 7 direct reads today).
- **Third runtime**: `observation_third_runtime_drives_roster_fleet_and_summary` keeps passing, and adding a runtime does not touch `RuntimeKind` beyond one variant.
- **Cost guard**: `session_observation_summary_v` stays under 50 ms on the largest Claude session (32 ms today).
