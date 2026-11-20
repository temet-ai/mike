---
title: The Prompt Workbench — a non-destructive editor for system-prompt alteration
status: draft
created: 2026-06-10
updated: 2026-06-10
summary: Design vision for the SPA's system-prompt editing surface — edit-by-example rule authoring, git-style diff and blame views, pipeline x-ray, token treemap, dry-runs against captured history, and a lazy-section simulator.
session_id:
  - bb7dad85-a608-4e27-b18a-c70e14d8a5d2
thread: lazy-prompt-sections
---

# The Prompt Workbench

> The system prompt is a program that runs on every request. Today we edit it blind —
> config files, rule forms, restart-and-pray. The proxy sees every byte of it, before and
> after every transformation, across every version Claude Code ships. That makes the proxy
> the one place in the world where a system-prompt **IDE** can exist.

This is the thing the capture pipeline has been building towards: not a viewer with a rule
form bolted on, but an editing surface where the operator sculpts what the model actually
receives — non-destructively, with the same instincts they bring to a code review.

Everything below is buildable from data the proxy already captures. Each feature names its
source primitive.

---

## 1. The core inversion: edit the prompt, not the rule

Today: operator reads the prompt, mentally computes a transformation, encodes it as a
`(kind, value, action, text)` tuple, hopes the matcher agrees.

Workbench: the operator opens the **live system prompt as a document** and *edits it*.
Select the Chrome section, hit delete — the workbench reverse-compiles the edit into a
`drop` slice rule. Select a paragraph, retype it — a `replace` rule. Type a new line after
a block — `append`. Select a section and hit "L" — a `lazy_section` rule with a stub
scaffold. The rule form still exists (it's the "view source" of an edit), but it's the
compiler output, not the authoring surface.

**Edit-by-example.** The diff between "what Claude Code sends" and "what you want Anthropic
to see" *is* the rule set. The operator never writes a matcher again; they make the edit
and the workbench derives the most robust matcher that produces it (exact slice →
heading-span → xml-tag, preferring structural matchers over literal ones, flagging when a
literal matcher would be version-fragile).

- Built from: `filter/matcher.rs` granularities; `request_body` as the source document.
- Editor: CodeMirror 6 + `@codemirror/merge` (Svelte-friendly, decorations API, an actual
  maintained merge/diff view — Monaco is 10× the weight for nothing we need).

## 2. `git add -p` for your system prompt

The natural review unit is the **chunk**, not the rule. The workbench's main view is a
merge view: left (or inline ghosts) = original captured prompt, right = post-pipeline
result. Every divergence is a chunk with accept/reject controls — staging an edit is
`git add -p`, the staged hunks are the pending rule mutations, and **Apply** is the commit
(rules table write + ArcSwap swap, exactly the hot-reload path that exists today).

Diff niceties that matter here specifically:

- **Word-level intra-line diffing** — prompt edits are prose edits; line-level diffs are
  unreadable for a reworded sentence.
- **Unified / split / ghost toggle** — ghost mode renders deletions as struck-through
  inline phantoms inside the single document, which is how prose editors think.
- **Chunk provenance chips** — every chunk carries a chip: which rule produced it, when,
  by whom (operator vs seeded), with a one-click "disable this rule" right on the chunk.

- Built from: `request_body` vs `request_sent_body` (the diff pair has existed since day
  one); `firing_rule_ids` already attributes chunks to rules.

## 3. Blame view — every line answers "why does the model see this?"

Gutter annotation on the *result* document: untouched lines are blank; transformed lines
show the rule chip; **injected** lines (lessons, reminders, lazy-section payloads,
composer) show the pipeline stage that added them. Hover a line → the full causal story:
"replaced by rule #12 (`lazy_section: # Claude in Chrome…`), created 2026-06-10, fired on
1,847 requests, last fired 14s ago."

This is `git blame` where the "commits" are pipeline stages and rules. It kills the
debugging session that starts with "where the hell did this sentence come from?"

- Built from: `firing_rule_ids`, the handler's stage ordering (filter → lessons →
  reminders → composer), reminder audit fields (`source_request_id`, `reminder_text`).

## 4. Pipeline X-ray — scrub through the transformation like layers

Pick any captured request. The workbench shows the pipeline as toggleable layers, Photoshop
style:

```
  ◉ original           366.9 KB
  ◉ filter             −4.3 KB   (2 rules fired)
  ◉ lessons            +1.1 KB   (session start)
  ◉ reminders          +0.4 KB   (3 findings from req 5221)
  ◉ composer           ±0
  ──────────────────────────────
    forwarded          363.9 KB
```

Toggle any layer off → the diff view re-renders what *would* have been sent without it.
Scrubbing a slider through the stages animates the document morphing stage by stage. This
is both a teaching tool and the fastest possible answer to "which stage broke this
request?"

- Built from: stage outcomes are already computed sequentially in `proxy/handler.rs`;
  the x-ray needs them *persisted per stage* (today only the final composite lands in
  `filtered_request_body`) — one new capture column or a recompute-on-demand endpoint
  that replays the pipeline against the stored original. Recompute is the honest,
  storage-free option and is deterministic by construction.

## 5. The weight map — make the 91k tokens visible

A treemap (or proportional sidebar outline) of the full request: every system-prompt
section by heading, the tools array by tool, skills list, memory, gitStatus — each cell
sized by token estimate, colored by **utilization** (did any request in the last N days
actually *use* this? did the session ever call a chrome tool? did the model ever invoke
that skill?).

Cold, heavy cells glow as lazy-section candidates. Click one → "make lazy" flow (§7).
A budget gauge across the top: current tokens, staged delta, projected per-request and
per-day savings at observed traffic volume.

- Built from: section parse of captured system blocks; `tools[]` byte sizes;
  `tools.uses` for utilization (which tools/skills actually fire per project);
  request volume from `requests.requests` for the $/day projection.

## 6. Dry-run against history — never ship a rule blind

The single most valuable safety feature: as the operator edits, a live panel replays the
draft rule set against the last N captured requests (per project):

```
DRY RUN — last 50 requests, project temet-proxy
  fires on        47/50
  no-ops on        3   (subagent dispatches — heading absent)  ✓ expected
  avg delta       −4,311 bytes  (~1,078 tokens)
  thinking risk    2 requests carry thinking blocks  ⚠ R1
  est. savings    ~52k tokens/day at current volume
```

Click any row → that request opens in the merge view with the draft applied. The
thinking-block tripwire (R1 from the lazy-sections design) is a first-class warning, not a
footnote: requests that would 400 are listed by id.

- Built from: stored `request_body` corpus + the pure filter functions (they're already
  `fn(&[u8], &cfg) -> outcome` — replay is a loop); thinking detection is a JSON-pointer
  scan. New inspect endpoint: `POST /api/rules/dry-run {rules, sample}` → per-request
  outcomes. MCP parity gives agents the same capability.

## 7. The lazy-section composer & trigger simulator

Authoring a lazy section is a *conversation design* problem, so the editor shows the
conversation. Selecting a section and choosing "make lazy" opens a split stage:

- **Left — the two artifacts**: stub editor (with live token counter and the trigger token
  highlighted) above the injected-reminder preview, both rendered exactly as the model
  will see them.
- **Right — a simulated session timeline**: request 1 (system shows the stub, section
  ghosted out), … the trigger turn (assistant says `!!CHROME!!`, calls
  `tabs_context_mcp`), the injection turn (the `<system-reminder>` block materializes
  in the tool-result message, position-pinned), and a later turn proving the position is
  byte-stable. Cache boundary markers (§8) drawn on the timeline show that no cached
  prefix ever invalidates.

The simulator runs the real `lazy_sections` code against a synthetic three-request
fixture — it is the integration test (R3) wearing a UI.

- Built from: the phase-1 `lazy_sections/` pure functions; `cache_control` positions in
  captured bodies.

## 8. Cache fault lines & cost honesty

`cache_control` breakpoints render as horizontal fault lines across the document. Any
staged edit that lands **above** a fault line gets an amber ribbon: "this edit invalidates
a 1h-TTL cached prefix (~89k tokens) — first request after apply pays full input price."
Edits below the line are green. The workbench refuses to let cache implications be
invisible, because invisible is how they bite.

- Built from: `cache_control` blocks in captured bodies; token estimates per block.

## 9. Time machine — diff Claude Code against itself

The proxy has seen every system prompt every Claude Code version ever sent through it
(`prompt_sha256` + compositions registry). The workbench grows a version axis:

- **Upstream diff**: "what changed between cc 2.1.169 and 2.1.170?" as a normal merge
  view. When Anthropic rewords the Chrome section, you see it the day it ships — and the
  heading-span matcher either survives (green) or the rule row gets a "matcher drifted"
  badge (amber) *automatically*, because the dry-run (§6) re-runs on new compositions.
- **Rule-set history**: rules already carry `updated_at`; surface the audit trail as a
  log with checkout — "show me the pipeline as it was last Tuesday," replayable against
  any captured request via the recompute endpoint (§4).

- Built from: `prompt_compositions` (sha256-keyed, versioned), `list_prompt_anomalies`
  (already classifies unknown/structural-mismatch prompts — the drift detector exists,
  it just doesn't talk to the rule editor yet).

## 10. Efficacy — close the loop with outcomes

Rules change model behaviour; the proxy *also* captures behaviour. A rule's detail page
shows its consequences, not just its firings: corrected-turn rate, tool-error rate, and
weasel-finding rate for sessions where the rule fired vs the project baseline
(`operator_turns` outcomes, `list_tool_errors`, diagnostic findings). Not a controlled
experiment — but a tripwire: if corrected-turns spike after a prompt edit, the workbench
says so on the rule that did it.

- Built from: `operator_turns.outcome`, `tools.results.is_error`, diagnostics — all
  joinable to firing rules by request id.

---

## What it looks like

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ⌬ PROMPT WORKBENCH        claude-code 2.1.170 · fable-5 · project temet-proxy│
│ budget ▓▓▓▓▓▓▓▓▓░ 91.4k tok   staged Δ −7.5k   est −52k tok/day              │
├────────────┬──────────────────────────────────────────────┬─────────────────┤
│ WEIGHT MAP │ MERGE VIEW (CM6)        unified·split·ghost  │ RULE STACK      │
│            │                                              │                 │
│ ▣ tools    │  # Claude in Chrome browser automation       │ ◉ 12 lazy §chro…│
│   25.5k ░░ │  ─ You have access to browser automation…    │   fired 1,847×  │
│ ▣ skills   │  ─ ## GIF recording …                        │ ◉ 09 drop xml…  │
│   13.2k ░  │  + Browser-automation guidance is available  │ ○ 07 replace …  │
│ ▣ chrome § │  +   on demand: output !!CHROME!! …          │   (disabled)    │
│   4.3k ◀━━ │       ┌────────────────────────────┐         │ ─────────────── │
│ ▣ memory   │       │ chunk · rule #12 · keep? ✓✗│         │ + staged (2)    │
│ ▣ output   │       └────────────────────────────┘         │   [Apply]       │
│   style    │  ══════ cache fault line (1h TTL) ══════     │   [Dry-run ▸]   │
├────────────┴──────────────────────────────────────────────┴─────────────────┤
│ DRY RUN  47/50 fire · −4.3 KB avg · ⚠ 2 thinking requests would 400 (R1)    │
│ X-RAY  ◉ original → ◉ filter → ◉ lessons → ◉ reminders → forwarded          │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Build order (each phase ships standalone value)

| Phase | Slice | New backend surface |
|---|---|---|
| W1 | **Merge view** on request detail: `request_body` vs `request_sent_body`, word-level diff, chunk → rule provenance chips | none — data exists |
| W2 | **Dry-run endpoint + panel**: draft rules replayed over captured corpus, thinking tripwire | `POST /api/rules/dry-run` (+ MCP tool + manifest entry) |
| W3 | **Edit-by-example**: editable merge view compiles edits → staged rules; `git add -p` staging; Apply = existing CRUD | rule-derivation logic (pure, frontend or backend) |
| W4 | **Weight map + budget gauge**; "make lazy" entry point | section/tool tokenization endpoint |
| W5 | **Lazy-section composer + trigger simulator** (lands with `lazy_sections/` phase 1) | simulator endpoint wrapping the pure fns |
| W6 | **Pipeline x-ray** (recompute-on-demand), **blame gutter** | `GET /api/requests/{id}/pipeline` replay endpoint |
| W7 | **Time machine + drift badges**, **efficacy panel** | compositions-diff + outcomes-join endpoints |

Constraints carried throughout: inspection-fidelity (full content everywhere — the merge
view *is* the anti-truncation answer: it scrolls, it never clips), virtualized rendering
for 366 KB documents, every new REST route gets its MCP twin via the manifest parity gate,
and `ui-designer` owns the visual language via Storybook stories before
`frontend-developer` builds components.

## Naming

The SPA surface is **Workbench** (route: `/workbench`). The rule stack panel is the
**Pipeline**. The dry-run panel is the **Wind Tunnel** if we're feeling ourselves, "Dry
run" if we're not.
