---
title: Prompt shaping — what only works on the wire request
status: active
created: 2026-10-28
updated: 2026-10-31
summary: For each filter kind, lazy section, reminder and composition, what only works on the wire request and why, with source lines, and where tether's prompt-shaping decision agrees or differs.
---

# Prompt shaping — what only works on the wire request

Paths are under `crates/mike/src/`. Line numbers are at the commit that adds this doc.
tether#3 asks which of mike's filter rules, lazy sections and compositions a client that
only sees Claude Code hooks can reproduce. `GET /api/sessions/{id}/prompt-shaping`
(`api/prompt_shaping.rs:130-141`) hands the rules out; this doc says which of them need the
wire request.

## What the route gives

`build` (`api/prompt_shaping.rs:106-128`) returns enabled, project-scoped rules split three
ways: `text_substring` and `xml_tag` as `filter_rules` (`:66-81`, scope
`system_and_messages`), `lazy_section` and `inject_reminder` as `lazy_sections` (`:83-97`),
and every cached composition (`:108-122`). The handler answers 400 for a non-UUID id,
then calls `api::client_serves::require_client_serves(&state, "client_serves_prompt_shaping")`
(`api/prompt_shaping.rs`, `api/client_serves.rs`), which answers 409 unless the global
`client_serves_prompt_shaping` feature toggle is on. The gate is one toggle for the whole
installation, not a per-session or per-kind mark, and no pipeline stage reads it. The filter
stage, the lazy-section stage and the composer always run, so a tether session
keeps mike's `text_substring` and `xml_tag` rules, its lazy sections and its compositions
(seen live on 2026-10-31: request 726379's filter stage `applied` rules 78, 58, 63, 64, 67,
69 and 76 in a tether session). `ProjectQuery` has only `project` (`:12-16`), so a `kinds`
parameter is ignored and the answer carries every kind; tether reads only the reminders.

## Four things only the wire request shows

1. **Global cache-prefix protection.** The system array carries cache-scope markers.
   `cache_scope::last_global_index` (`cache_scope.rs:11-13`) returns the last globally scoped
   block. The filter walks `system` with `protect_global_prefix = true`
   (`filter/content_blocks.rs:36-37`) and `walk_blocks` copies every block at or before that
   index untouched (`filter/block_walk.rs:18-26`). Lazy sections apply the same boundary when
   stripping (`lazy_sections/mod.rs:141-145`). The marker exists only in the request bytes;
   a rewrite there would change a prefix Anthropic caches globally.
   `verify_global_prefix_preserved` (`cache_scope.rs:27-33`) checks it.
2. **First-request rewrite and replay.** The filter rewrites `system` only on a session's
   first primary request (`TurnOrigin::PrimaryFirst`, `filter/stage/mod.rs:239-240`;
   `apply_fresh` `:17-53`). It captures what it did with `SystemReplay::capture`
   (`:173-188`, `filter/replay/mod.rs:114`). Every later request splices the captured rewrite
   (`apply_replay`, `filter/stage/mod.rs:55-93`; `SystemReplay::apply`,
   `filter/replay/mod.rs:146`) even when the rule set has since changed (`:60-64`, log at
   `:190-196`), so the system prefix stays byte-identical for the prompt cache. After a
   restart `recover` rebuilds the capture (`:97-123`). A hook cannot do this: it needs the
   exact bytes sent last time and the comparison against this request's `system[]`
   (`:65-73`, decline `filter_replay_system_mismatch`).
3. **Thinking-block guard.** `apply_lazy_sections` returns `Unchanged` when any message holds a
   `thinking` or `redacted_thinking` block (`lazy_sections/mod.rs:108-113`, detector
   `:190-207`). Anthropic signs thinking blocks over the conversation before them, so
   rewriting `system` or inserting text into history invalidates them. The guard covers
   `inject_reminder` too, because both kinds go through `apply_lazy_sections`
   (`:23-34`). Only the message array in the wire request shows whether a thinking block is
   present.
4. **Matching inside `tool_result` and message history.** `apply_request_filter` walks the
   `content` array of every message with protection off (`filter/content_blocks.rs:41-50`).
   `walk_blocks` only transforms blocks where `is_text_block` is true
   (`filter/block_walk.rs:27-30`, `filter/content_blocks.rs:121-127`), so matching covers
   text blocks of every past user and assistant turn. Claude Code's hooks report the current
   prompt and tool calls, not the stored history that is resent each turn.

## Per kind

| Kind | Wire-only because | Source |
|---|---|---|
| `text_substring` | Case-insensitive match and slice rewrite of any `system` text block outside the global prefix and of every message text block (`MatchSpan::Slice` drop, replace, append). Needs the block text as sent, the global boundary (1), the replay (2) and history (4). | `filter/block_walk.rs:86-96`, `:123-133`; `filter/matcher.rs:8-16` |
| `xml_tag` | Whole-block match (`MatchSpan::Whole`) on a tag in `system` or history text blocks; same four constraints. A block that becomes empty is dropped (`:137-139`). | `filter/block_walk.rs:87`, `:108-122`, `:137-139`; `filter/matcher.rs:9` |
| `lazy_section` | Strips a heading's span from a non-global `system` text block, replaces it with a stub, and injects the section after a trigger. Needs the system text with headings (`find_heading_span`), the global boundary (1) and the thinking guard (3). A client that injects without stripping sends the text twice. | `lazy_sections/mod.rs:62-67`, `:139-172`, `:108-113` |
| `inject_reminder` | Appends a wrapped `text` block to the first user message after the latest assistant message whose text contains `trigger_token`. The trigger is read from message history, and the guard (3) applies. The text is never matched against the system prompt. | `lazy_sections/mod.rs:68`, `:73-88`, `:174-179`; `lazy_sections/injector.rs:3-41` |
| composition | Selected by the SHA-256 of the system prompt as Claude Code sent it: the hash is parsed from the pre-filter body (`compose_prompt/mod.rs:90-99`; `classify_and_compose` gets both bodies, `compose_prompt/stage.rs:56-66`), so a filter rewrite does not change which composition matches. `observe` passes through (`compose_prompt/decision.rs:104-106`), `template` rewrites (`:107-110`), `replace` is not enabled in V1 and falls back to passthrough as `replace_unsupported` (`compose_prompt/mod.rs:193-200`). The prompt bytes, and the global-prefix rule for template splices, exist only in the request. | as cited |

## Agreement with tether's `docs/decisions/prompt-shaping.md`

Compared against the tether mod's `docs/decisions/prompt-shaping.md` (U3).

Agree:
- `text_substring`, `xml_tag`, `lazy_section` and composition `template`/`replace` are not
  applicable from a hook-level client. Both docs give the same reasons: block text and history
  are not visible, the strip needs headings, the hash is of the sent system block.
- The thinking-block skip makes lazy sections wire-only (tether row `lazy_section`; mine
  item 3).
- The global cache prefix, first-request rewrite with replay, and history matching are wire-only
  for every filter rule (tether's "Also wire-only" paragraph; mine items 1, 2, 4).
- The route is gated by one global toggle, not a per-session or per-kind mark, and ignores
  `kinds` today (tether's Consequence section; `api/prompt_shaping.rs`). Observe-mode compositions
  change nothing.

Differ:
- tether applies `inject_reminder` as a standing section `shaping:<rule id>` and says the text
  "is never matched against anything". mike's `inject_reminder` is conditional: it fires only
  when the latest assistant text contains `trigger_token` and is skipped on thinking blocks
  (`lazy_sections/injector.rs:30-41`, `lazy_sections/mod.rs:73-88`, `:108-113`). A standing
  section is a different behaviour: it ignores the trigger, so tether's version fires on
  every prompt. The route does not return a `kind` filter to make that safe: `trigger_token`
  is in the payload (`api/prompt_shaping.rs:37`).
- The reminder is appended as a message block, after the cache boundary in `messages`; tether's
  section is "after the cache boundary" in the prompt, a different position that mike's
  system-prompt-cache rule also permits only as a new tail block.
- tether says template/replace "No". mike's `replace` mode does not run at all in V1
  (`compose_prompt/mod.rs:193-200`), so only `template` is a real loss when tether takes the
  stage.
- tether's decision depends on mike reading `kinds` and marking per kind. That is still not
  built: the handler only checks the global `client_serves_prompt_shaping` toggle
  (`api/prompt_shaping.rs`).
  Until it is, a tether session that reads the route for reminders loses every other kind
  above. That is the same dependency tether recorded, now confirmed on main.

## Revisit when

The route takes `kinds` and the client-serves gate works per kind, or Claude Code's hooks report
system sections and message text as rewritable.
