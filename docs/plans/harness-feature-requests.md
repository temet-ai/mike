---
title: Harness feature requests — from the tool-call efficiency investigation
status: draft
created: 2026-07-02
updated: 2026-07-02
summary: Five capabilities that can't be built as a repo script — anchored Edit, batch Read/Edit, WebFetch section/grep, cross-subagent deferred-tool schema persistence, and client-side schema pre-validation — with the waste each would kill.
---

# Harness feature requests

Items below need Claude Code harness support, not a repo script — each maps to a report proposal marked "harness FR" in `artifacts/2026-07-02/tool-call-efficiency/report.md`.

## 1. Anchored Edit (edit-at-pattern)

**What:** an `Edit` variant that locates its target by anchor pattern (regex/symbol) instead of exact line-numbered text, so a prior Edit in the same turn doesn't invalidate the next one's line numbers.

**Observed waste:** the report's §5 rg-locate → Read-offset → Edit loop over `tools.rs`/`client.rs` cost ~44 calls (session 08601663, reqs 80743–80782) purely because every Edit shifted subsequent line numbers, forcing a re-locate before each next edit.

**Proposed shape:** `Edit(file, anchor_regex, old_string, new_string)` — resolve `anchor_regex` to a byte range first, then apply the existing exact-string match within that range. `scripts/read-anchored` (shipped this run) covers the read-side half; the edit-side half needs harness support.

## 2. MultiEdit / batch Read

**What:** multiple edits to one file, or multiple files read, in a single tool call.

**Observed waste:** 6 sequential single Edits to one file that one batch edit would collapse (b62d1ba1, reqs 81524–81525); 3 structurally identical conflict-hunk Edits (3797d508); 9 slice-reads of one file collapsible to one call (c36fc7c4); 7 sequential Reads collapsible to one (5fbfac59).

**Proposed shape:** `MultiEdit(file, [{old, new}, ...])` applied atomically in one call; `Read(file_paths: [...])` or `Read(file, ranges: [[a,b],...])` for the batch-read side.

## 3. WebFetch `section`/`grep` param

**What:** a filter parameter on `WebFetch` so a fetched page/doc can be projected down to matching sections before it enters context, the same way `scripts/spill-query` (shipped this run) does for local spill files.

**Observed waste:** research sessions spilled a fetched page then greped the spill by hand (§3/§6): 2 spills + 5 grep variants in one session (08601663) to answer questions the fetch itself could have answered directly with a `grep`/`section` param.

**Proposed shape:** `WebFetch(url, grep: "pattern", context: N)` or `WebFetch(url, section: "heading")`, returning only matching spans plus a byte-count note for the rest (inspection-fidelity-compatible: full content still available via a follow-up unfiltered fetch).

## 4. Deferred-tool schema persistence across subagents

**What:** once a deferred MCP tool schema is loaded via `ToolSearch` in one subagent/lane, sibling subagents in the same fan-out should not have to reload the identical schema set.

**Observed waste:** `ToolSearch` reloaded the identical schema set 14× across subagents of one session (a16dd53f); 9× in this investigation's own run.

**Proposed shape:** a fan-out-scoped schema cache keyed by tool name, populated on first load and shared read-only to sibling subagents dispatched from the same parent turn.

## 5. Client-side schema pre-validation before send

**What:** validate a tool call's arguments against its declared schema before the call leaves the client, so a malformed call fails locally instead of round-tripping to get a validation error back.

**Observed waste:** identical-argument retries after a validation error round-trip (08601663, d5b4c22a) — the same malformed call is re-sent with no argument change because the fix was only visible after a full round-trip.

**Proposed shape:** schema-check `tool_use` blocks against the tool's JSONSchema client-side; on failure, surface the validation error to the model without a network round-trip, retry-count still incremented so runaway retries are visible.
