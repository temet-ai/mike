---
title: rtk (Rust Token Killer) — competitive analysis & transferable ideas
status: active
created: 2026-06-15
updated: 2026-06-15
summary: How rtk-ai/rtk reduces agent context bloat by filtering tool-command output, and which of its design ideas are worth adopting in temet-proxy.
session_id:
  - 23214930-40a5-41bd-b773-3d13e30b10b8
---

# rtk (Rust Token Killer) — competitive analysis

Researched [`rtk-ai/rtk`](https://github.com/rtk-ai/rtk) via three parallel read-only
agents (overview/hook mechanism, core filter engine, analytics/economics) on 2026-06-15.
This doc captures how it works and what is worth borrowing for temet-proxy.

## What rtk is

A small, fast Rust CLI (<10ms startup, no async, no LLM in the hot path) that
**intercepts the shell commands an AI coding agent runs and strips boilerplate from
their output** before it re-enters the model's context window. `cargo test`'s 200 noisy
lines collapse to an error summary; `git push`'s 15-line "Enumerating objects…" becomes
`ok main`. Headline claim: a 30-minute session drops from ~118K to ~24K tokens (~80%).

### Relationship to temet-proxy

rtk is the **mirror image** of this project:

| | temet-proxy | rtk |
|---|---|---|
| Position | On the wire (reverse proxy) | On the agent (command wrapper + hook) |
| Filters | The **request** (system prompt + message history) going up | The **tool result** coming back down |
| Primary mode | Inspect + measure + steer | Reduce output tokens |
| Shared disease | Context bloat | Context bloat |

They attack the same problem from opposite ends of the pipe and are complementary, not
competitive — both could run simultaneously (rtk on the agent, temet-proxy on the wire).

## How rtk works (three parts)

### 1. Rewrite hook (the on-ramp)

A `PreToolUse:Bash` hook calls `rtk rewrite "<cmd>"`, which regex-matches against one
static rule table (`src/discover/registry.rs`) and rewrites `npm test` → `rtk npm test`,
returning it via `permissionDecision: allow`. Exit-code protocol: `0`=rewrite,
`1`=passthrough, `2`=deny, `3`=ask. **All mapping lives in one binary** — every agent
integration (Claude Code, Cursor, OpenCode TS plugin, Gemini, Codex via AGENTS.md,
Windsurf/Cline via rules files) is a thin shell delegating to `rtk rewrite`. Adding a
command is one rule entry; all agents benefit at once.

### 2. Filter engine

Two pipelines:

- **TOML-driven** (most commands): 8 ordered stages — strip-ansi → regex replace →
  `match_output` short-circuit → strip/keep lines → truncate → head/tail → max_lines →
  on_empty.
- **Rust-native streaming** (`StreamFilter::feed_line`) for high-value tools (cargo, git)
  with stateful block detection.

Filters are `*.toml` files concatenated at build time and embedded in the binary. Each
file carries inline `[[tests]]` that run as `cargo test` — **a filter with no test or a
wrong expected output fails the build.** Project-local `.rtk/filters.toml` is gated behind
`rtk trust` + a content hash (re-disabled silently if the file changes after trust).

Truncation always emits an accurate `... (N lines omitted)` marker; failed commands tee
their full raw output to `~/.local/share/rtk/tee/<epoch>_<cmd>.log` with a
`[full output: <path>]` hint. (Successful commands have **no** tee by default — see
Caveats.)

### 3. Economics

Every run records raw vs filtered byte counts to local SQLite. `rtk gain` shows token/USD
savings; `rtk discover` / `rtk session` scan Claude Code session JSONLs to find commands
run *without* rtk — a backlog of missed savings. USD figures use a **weighted
cost-per-token** model (below).

## Ideas worth taking

Several rtk decisions converge on conclusions we reached independently (validation); a few
are genuinely new.

### 1. Tool-call cost attribution → better-call suggestion (strongest idea)

rtk mines session transcripts for recurring un-optimized commands and rewrites them via an
agent hook. We are on the wire, not the agent, so we **suggest** rather than rewrite — but
we already capture the data to attribute token cost per tool call, and can measure it more
honestly.

`tools.results.content_bytes` (per-call output size) joined to `tools.uses_v.command`
(parsed bash command) tags every tool call with a token cost today. Verified 2026-06-15:
~730M est. tokens of tool output captured; `grep` alone is 65K calls / ~17M tokens.

The metric that beats rtk is **amortized carry-forward cost**: rtk measures one-shot output
reduction at the hook, but a tool result becomes input tokens in the next request and is
re-sent every turn until compaction. We have the whole conversation, so we can compute how
many later requests re-paid for a result — the honest "you could save Y tokens" figure,
which rtk structurally cannot produce. Build shape and the system-prompt-block variant are
in [issue #164](https://github.com/temet-ai/mike/issues/164) (item 20.1).

### 2. Weighted cost-per-token for honest USD savings

rtk normalizes token types into input-equivalent units using real pricing ratios
(`output ×5, cache_create ×1.25, cache_read ×0.1`), derives cost-per-input-token from
ccusage spend, then multiplies by saved tokens. Bolting this onto our compression-
measurement subsystem would let us report the **dollar value of each filter rule** —
far more legible than "19% off the tool catalogue."

### 3. Their fidelity rule is our fidelity rule

rtk's hard rule — *"never show `…+N more` without a recovery path"* — is verbatim our
[`inspection-fidelity.md`](../../.claude/rules/inspection-fidelity.md). Independent
convergence on the same principle; their tee-on-failure pattern is a clean reference
implementation. Nothing to copy, strong external corroboration.

### 4. Value-prop CI gate

`scripts/benchmark.sh` fails CI if any filter produces *more* tokens than raw input, or if
fewer than 60% of cases save ≥60%. We have `dry_run_filter_rules` (measurement) but no
gate — we could fail a check if a filter rule ever *increases* request bytes (a real risk
with `append`/`replace` rules).

### 5. `match_output` short-circuit with `unless` guard

Collapse a verbose success blob to `ok` *unless* an error pattern is also present — a
generalization of our lazy-section idea applied to bodies, with a guard against swallowing
mixed-signal output.

## Caveats before borrowing

- **Token counting is `chars/4` heuristic** (±10%, byte-length so multibyte overcounts).
  Fine for trends, not billing. Use a real tokenizer if we surface dollar figures.
- **`local_llm.rs` / `summary.rs` are misleadingly named** — no LLM, just regex
  heuristics.
- **rtk loses data on success**: `max_lines` truncation on successful commands has no tee
  by default. That is exactly the trap our fidelity rule forbids; if we adopt any of their
  filtering, keep our stricter escape-hatch default.

## Source map (rtk-ai/rtk)

- Rewrite rules / single source of truth: `src/discover/registry.rs`, `src/discover/rules.rs`
- Hook scripts: `.claude/hooks/rtk-rewrite.sh`, `hooks/<agent>/`
- Filter engine: `src/core/{toml_filter,runner,stream,tee,truncate,filter}.rs`, `src/filters/*.toml`
- Economics: `src/analytics/{cc_economics,ccusage,gain,session_cmd}.rs`, `src/core/telemetry.rs`
