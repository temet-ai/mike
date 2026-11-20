---
title: Lazy Prompt Sections — strip rarely-used system-prompt sections, inject on demand
status: active
created: 2026-06-10
updated: 2026-06-19
summary: Replace the always-present Claude-in-Chrome system-prompt section with a one-line stub; when the model asks for it (trigger token), inject the full section as a system-reminder block — stateless, cache-stable, version-drift-proof.
session_id:
  - bb7dad85-a608-4e27-b18a-c70e14d8a5d2
thread: lazy-prompt-sections
---

# Lazy Prompt Sections (Phase 1: Claude in Chrome)

> **Update 2026-06-19 — once-per-context-window + marker-derived wrapper.**
> Injection now fires only when the trigger marker is in the **most recent
> assistant turn** (not the first occurrence anywhere in history). The
> reminder therefore loads **once** — on the request right after the model
> asks — instead of riding along in every subsequent request; the model
> re-emits the marker to reload it. The injected block is wrapped in a tag
> derived from the marker (`<chrome>` -> `<chrome>...</chrome>`), falling back
> to `<system-reminder>` for non-tag markers like the legacy `!!CHROME!!`.
> Trade-off vs the old "stable position" design (§R3): dropping the
> reminder on the turn after it loaded invalidates the message cache once
> at that point, in exchange for not re-sending the section every later
> request — a net win on long sessions. Implemented in
> `crate::lazy_sections::{injector,wrap_reminder}`; semantics below describe
> the original first-occurrence design and are superseded by this note.

## Problem

Every Claude Code request carries content most sessions never use. Measured on captured
request 5222 (this repo, 2026-06-10):

| Where | Size | Tokens (est.) |
|---|---|---|
| `# Claude in Chrome browser automation` section, inside system block 2 | ~4.3 KB | ~1.1k |
| 22 `mcp__claude-in-chrome__*` tool definitions in `tools[]` | ~25.5 KB | ~6.4k |

Phase 1 targets the system-prompt section only. The tools array is Phase 2 (deferred — see §9).

## Concept

1. **Strip**: the proxy replaces the Chrome section in the outgoing system prompt with a stub:

   > Browser-automation guidance is available on demand: output `!!CHROME!!` in your text,
   > then call `mcp__claude-in-chrome__tabs_context_mcp` — the full instructions arrive as a
   > system-reminder with the tool result.

2. **Trigger**: when any assistant message in the replayed history contains `!!CHROME!!`,
   the proxy injects the full section as a `{type:"text"}` `<system-reminder>` block into the
   user message immediately following the first trigger occurrence.

3. **Re-extraction, not storage**: Claude Code resends the complete system prompt on every
   request. The proxy strips it every time — and at injection time reads the section text
   straight out of the *current* request's own system block. No persisted copy, no staleness
   across Claude Code version bumps, nothing to migrate.

## Why this shape

- **The trigger token in replayed history IS the state.** Claude Code never sees proxy-side
  rewrites, so the proxy must re-strip and re-inject deterministically on every request.
  Scanning messages for the first `!!CHROME!!` and injecting at a fixed position relative to
  it is stateless, idempotent, survives proxy restarts, and needs no session table.
- **Cache-stable.** The stripped system prompt is identical for the whole session (stub is
  constant), so the system/tools cache prefix never changes. The injected block lands after
  the trigger message — at the tail on the triggering request, then at the *same byte
  position* in every replayed history afterwards, so the incremental message-prefix cache
  stays warm.
- **Fail-safe.** Following `filter/` discipline: any parse failure, missing heading, or
  malformed shape folds into "forward original bytes". The proxy never drops a request
  because of this feature.

## Existing machinery this builds on

| Piece | Where | What it already does |
|---|---|---|
| Slice-level system-block rewrite | `crates/claude-proxy/src/filter/` (`mod.rs`, `matcher.rs`, `content_blocks.rs`) | `TextSubstring` rules drop/replace/append a substring inside a system block, first occurrence, sequential rules, fail-safe `Unchanged` |
| `<system-reminder>` injection into messages | `crates/claude-proxy/src/reminders/appender.rs` | Pure JSON walk appending a text block to the last **user** message; role gate (never touches assistant turns); conservative `None` on any shape surprise; no `cache_control` on the appended block |
| Pipeline hook + audit | `crates/claude-proxy/src/proxy/handler.rs:323-433` | Order: filter → lessons → reminders → composer. Every mutated body lands in `filtered_request_body` with `filter_applied = true`; original always in `request_body` |
| Rule CRUD + hot reload | `rules.rules` table, ArcSwap snapshot, MCP `*_filter_rule` suite, SPA per-block badges | DB-only rule state, swapped on every mutation |

What is genuinely new:

1. **Heading-span matcher** — match "from `# <heading>` to the next `\n# ` heading (or
   block end)" instead of a literal substring. A literal-substring rule would need the full
   4.3 KB section as its value and silently stop matching on any Claude Code wording change;
   a heading-span survives content drift inside the section.
2. **Trigger-conditional injection** — scan `messages[*].content[]` assistant text for the
   trigger token; inject adjacent to it (not only at the tail, since on later requests the
   trigger is mid-history).

## Design

### Rule shape

Extend `FilterRule` / the `rules.rules` table with one new kind and one nullable column:

- `kind = "lazy_section"` (new `FilterKind` variant)
- `value` = the heading line to match, e.g. `# Claude in Chrome browser automation`
- `text` = the stub that replaces the section (operator-authored, carries the trigger token)
- `trigger_token` = e.g. `!!CHROME!!` — new nullable TEXT column, used only by `lazy_section`
- `enabled`, `project_path`, ordering: unchanged semantics

One rule row drives both halves (strip + inject). CRUD rides the existing
`create/update/delete_filter_rule` API + MCP tools; the manifest/parity test forces the MCP
shape update in the same PR.

### Request-path algorithm (new step 1e.2, immediately after the filter, before lessons)

For each enabled `lazy_section` rule:

1. Walk `system[]` text blocks (array-shaped only, same scope rule as the filter). Find the
   heading span: from the start of the line equal to `value` up to (exclusive) the next line
   starting `# `, or end of block. Not found → rule is a no-op (fail-safe; Claude Code
   version moved the section → traffic flows untouched).
2. Extract `section_text` (full bytes of the span). Replace the span with `text` (the stub).
   A block left empty/whitespace-only is dropped (Anthropic 400s empty text blocks — same
   rule as filter slice edits).
3. Scan `messages[*]` with `role == "assistant"`, array-shaped content, `type == "text"`
   blocks, for the **first** occurrence of `trigger_token`.
   - Not found → done (stripped only).
   - Found in message *i* → inject `<system-reminder>\n{section_text}\n</system-reminder>`
     as a new `{type:"text"}` block appended to the content of the first `role == "user"`
     message at index > *i*. (On the triggering request that user message is the last
     message — the tool-result turn; on every later request it is the same mid-history
     message, so position is stable.) No user message after *i* → skip injection this
     request (next request will have one).
   - First occurrence only — later repeats of the token are ignored, so the model
     re-mentioning `!!CHROME!!` never duplicates the block.
4. Audit exactly like the filter: rewritten bytes → `filtered_request_body`,
   `filter_applied = true`, firing rule id recorded, `tracing::info!` with
   `stripped_bytes` / `injected` fields.

Implementation home: a new `crates/claude-proxy/src/lazy_sections/` module (mirroring
`reminders/` — `matcher.rs` for the heading span, `injector.rs` for the message walk, pure
`fn(&[u8], &Rule) -> Option<Vec<u8>>` cores), called from `handler.rs` between the filter
and lessons steps. It does not literally extend `filter/` internals because the
messages-scan + conditional-inject flow doesn't fit the per-system-block rule loop, but it
shares the same audit columns and rule table.

### Stub wording (initial)

```
# Claude in Chrome browser automation

Browser tools (mcp__claude-in-chrome__*) are available, but their usage guidance is
loaded on demand. Before your FIRST browser action in this session: output the token
!!CHROME!! in your text, then call mcp__claude-in-chrome__tabs_context_mcp. The full
guidance arrives as a <system-reminder> alongside the tool result. Do not use other
browser tools before reading it.
```

~340 bytes replacing ~4.3 KB → ~4 KB (~1k tokens) saved per request in non-browser sessions.

### The turn-end caveat

If the model outputs `!!CHROME!!` and ends its turn with plain text, no next request happens
until the user speaks — the injection arrives late. The stub mitigates this by pairing the
token with a mandatory cheap tool call (`tabs_context_mcp`), which the Chrome guidance
requires first anyway. In agentic flow the injection is therefore effectively synchronous.

## Risks

### R1 — body reserialization vs extended-thinking signatures (the big one)

`filter/mod.rs:63-79` documents that any `Rewrote` path emits `serde_json::to_vec` of the
whole parsed body (key order not preserved — `preserve_order` is NOT enabled in any
manifest, checked 2026-06-10), and Anthropic verifies `thinking` block signatures; filter
rules firing on extended-thinking traffic risk upstream 400s. A `lazy_section` rule fires on
**every main-loop request**, so this must be verified empirically before enabling by
default: run one session with extended thinking + a strip rule active and confirm the
upstream accepts replayed thinking blocks. The existing reminders/lessons appender has the
same reserialization exposure — any operational evidence from it transfers. If 400s occur,
this feature is gated to non-thinking traffic (detect `thinking` blocks in messages → skip
rewrite for that request; the section then rides in normally — graceful degradation).

### R2 — trigger-token collision

`!!CHROME!!` appearing in ordinary content (e.g. this design doc quoted in a message!) would
fire the injection. Acceptable: the consequence is one redundant ~4 KB reminder block, not
corruption. The scan is assistant-text-only, which excludes file contents echoed in tool
results (those live in user-role tool_result turns). Keep the token weird; make it
per-rule-configurable.

### R3 — section content shifts position across cache boundaries

On the triggering request, system shrinks (already stable — it was stripped from request 1)
and the reminder appears at the message tail — no cached prefix is invalidated at any point.
Verified reasoning, but add an integration test asserting byte-stability of the replayed
prefix across three consecutive synthetic requests (pre-trigger, trigger, post-trigger).

### R4 — subagents

Sub-agent dispatches carry different system prompts; the heading simply won't match (no-op)
unless the agent also has the section. Scope rule rows by `project_path` as today; no
special-casing.

## What this does NOT do (Phase 2+, deferred)

- **Tools-array stripping (the 25.5 KB).** Tool definitions cannot be system-reminder-injected:
  `tool_use` validates against `tools[]`, and mutating `tools[]` mid-session invalidates the
  entire prompt cache. Requires a per-session decision at request 1, held stable for the
  session (e.g. strip Chrome tools unless the first user prompt mentions browser work) plus
  an un-strip story (model asks → proxy returns a synthetic "restart with chrome enabled"?).
  Separate design when Phase 1 has soak time.
- **Other lazy sections.** The skills list (~13 KB in the same request) is the next
  candidate; the machinery is generic from day one (rule-driven, no Chrome-specific code).

## Test plan

Unit (in `lazy_sections/`):
- heading-span matcher: found mid-block / at block end / heading absent / two sections /
  heading inside a fenced code block (must not match — require line-start `# ` outside
  fences, or accept the simpler line-start rule and document it).
- injector: trigger in last assistant turn → appended to last user message; trigger
  mid-history → appended to the correct mid-history user message; trigger present twice →
  single injection; no user message after trigger → no-op; string-shaped content → no-op;
  assistant-role-only scan (token in a user message must NOT fire).
- strip leaves `cache_control` and sibling blocks untouched; empty-block drop.

Integration (`tests/integration/` or `tests/proxy/` with wiremock):
- end-to-end: request 1 stripped (stub present upstream, original in `request_body`,
  rewrite in `filtered_request_body`); request 2 with trigger in history → stripped AND
  injected; request 3 → identical injection position/bytes as request 2 (R3 assertion).
- rule disabled / heading absent → byte-identical forward (`FilterOutcome::Unchanged`-path
  parity).

Live verification before "done" (per project verification rules):
- enable on a real session, capture via the `mike-mcp` request tools, show stub upstream + injection firing;
- R1 thinking-signature smoke test.

## Implementation order

1. Migration: `trigger_token` column on `rules.rules` + `FilterKind::LazySection` variant +
   CRUD/MCP/manifest/SPA-badge plumbing (db-dev + rust-developer).
2. `lazy_sections/` module: matcher + injector + handler wiring (rust-developer).
3. Tests as above; `make fidelity` self-check (the stub replaces content for the *model*,
   not for inspection — original bytes stay in `request_body`, so the fidelity rule is
   satisfied; cite this in the PR).
4. Seed the Chrome rule, soak on this repo's own traffic, measure via captured requests
   (compare `original_len` vs `filtered_len` tracing fields / `compression_stats`).
