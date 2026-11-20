---
title: Proxy-Side Read Dedup (G1) — Design Only, No Implementation
status: draft
created: 2026-07-02
updated: 2026-07-02
summary: ADR-style design for extending the proxy's outgoing-body mutation pipeline to
  suppress redundant tool_result content (duplicate full-file Reads, cat/cat -n dumps,
  subsumed offset-windows, cross-lane duplicates). Corrects the tool-call-efficiency
  report's framing of the observed "Wasted call" synthetic result: that behavior is
  harness-native, not proxy code. Design only — operator decision: no implementation
  in this pass.
---

# ADR — Proxy-Side Read Dedup (G1)

> **Status: DRAFT.** Design only, per explicit operator instruction. No code, no
> toggle, no migration ships with this document. Implementation is a future PR
> gated on this design being reviewed and approved.

## Context

`artifacts/2026-07-02/tool-call-efficiency/report.md` §1 ("Duplicate full-file
re-materialization") measured, over 92 sessions / 10,893 tool uses in 3 days:

- Cumulative `Read` bytes: **18.3 MB** across 2,552 calls (avg 7.2 KB).
- 13 of the top-15 repeated-identical-call offenders are byte-identical full-file
  Reads, same session, same path, no offset/limit — up to 19x on one file.
- Named-session waste: 80c8b55c **1.58 MB ≈ 396K tok** (54% of session bytes,
  `gitlab_ci.py` read 22x); b6fbe0ae **~1.28 MB ≈ 320K tok** (122 redundant
  identical Reads); a16dd53f **~420 KB ≈ 105K tok** (`digest-ci.md` x7); a519d811
  **~397 KB ≈ 99K tok**; d65fa693 **51% of session bytes** via four whole-file
  `cat -n` dumps of `.gitlab-ci.yml`.
- Report's own total: **~3.5 MB / ~900K tok observed across 7 sessions in 3 days**
  (proposal G1's cited figure).

### Correcting the report's framing

The report's §1 closing line says: *"The defense partially exists already: the
proxy's wasted-call interception fired live in session 5fbfac59 — req 83906's
identical re-Read of `steps/analyze.py` returned a 97-byte synthetic 'Wasted call
— file unchanged since your last Read' instead of 3.6 KB of bytes."* This reads
as proxy behavior. **It is not.** Verified this session:

- `rg -n "wasted" -i crates/ .claude/` returns zero hits describing this behavior in `claude-proxy`/`proxy-mcp` source, hooks, or filter rules (the three matches that exist are an unrelated LSP-skill line, a cron-cost comment, and a DB round-trip comment).
- No filter rule of kind `duplicate_read`/`wasted_call` exists in the seed set or `filter/mod.rs`'s `FilterKind` enum.
- The synthetic text is produced by **the Claude Code harness itself**, client-side, when it re-issues a `Read` for a path/offset already in its own working set, unchanged since the last identical Read in the *same session*. The proxy never sees the tool call in that case — the harness never emits the request.

This matters for scope: the harness's existing dedup is real, but narrow —
**intra-session, identical-input only** (same path, same offset/limit, no
intervening Edit/Write in the harness's own view). It does not, and structurally
cannot, cover: cross-lane/subagent duplication (each lane is its own harness
context, but a proxy sees all of them on the wire), `cat`/`cat -n` via Bash
(harness Read-tracking is bypassed — d65fa693's own failure mode), offset-window
subsumption (a full read followed by a narrower Read of the same unchanged
file), or post-Edit re-reads (harness correctly does NOT dedup these — the file
legitimately changed — but the proxy could serve a smaller diff-shaped answer).
These four classes are where the report's remaining megabytes live, and are
the actual gap G1 should target.

## Decision space: where the proxy can intervene vs what stays harness-side

The proxy already mutates the **outgoing request body** before it reaches
Anthropic, in a fixed pipeline stage (`crates/claude-proxy/src/proxy/handler.rs`):
capture `stored_body` (original inbound bytes, pre-mutation, ~line 308) → filter
rewrites (`filter::service`/`filter::matcher`, kind/value/action against
`ArcSwap`-cached state) → lazy-section re-injection → `<mem:recall>`/lesson
injection → steerer reminder injection → composer splicing → forward — the
same stage `docs/adr/0001-proxy-steered-session-grounding.md`'s steerer
reminders and the report's cited filter-rule infrastructure already use.

**What the proxy CAN do** (this pipeline stage, request direction only): rewrite
a `tool_result` content block in an outgoing request — when Claude Code sends
back the result of a `Read`/Bash-`cat`/etc. tool call as a `user` message turn,
the proxy can detect that the result duplicates a prior turn's content and
splice in a short stub instead, before that turn goes upstream. Mechanically
identical to a filter-rule content rewrite: same request-body JSON-block
substitution, same pipeline point, same "capture stores the unmutated
original" guarantee.

**What the proxy CANNOT do**: intercept the tool call before the harness
executes it (the proxy never sees `Read` execute — only the *result* sent back
as a subsequent request body), or change what the harness already showed the
model client-side. The rewrite only changes what crosses the wire to
Anthropic, i.e. what enters the *next-turn* prompt history sent upstream. The
saving is real (fewer tokens billed and re-processed on every later turn that
includes that history), but the harness's own local echo to the
user/transcript is unaffected — a wire-level, not harness-level, optimization.

**What stays harness-side**: the intra-session identical-Read case the harness
already covers (no proxy action needed — redundant, not additive) and true
tool-call suppression (skip the Read from executing at all) — the proxy
cannot stop a call already in flight to the local filesystem/Bash; it can
only rewrite what gets *reported upstream* about that call's result.

## Design

### Dedup detection

Per-session index, keyed by `(file_path, content_sha256)`, populated from
every prior `tool_result` block for `Read`/`Bash` tool_use turns in that
session's outgoing bodies (same session_id the proxy already partitions
steerer/recall state by). On each new outgoing request: walk the newest `user`
turn's `tool_result` blocks; for each, resolve the paired `tool_use` (Read
`file_path`, or a Bash `command` parsed as `cat`/`cat -n <path>`) from the
same body — tool_use and result are already co-located, no cross-request
join needed; hash the result content and look up `(file_path, sha)`. On a hit
with no intervening invalidation, replace `content` with a stub (below); on a
miss, index the new hash and pass the block through unchanged.

### Invalidation

Conservative-by-design — a false "still valid" is a correctness bug (stale
content fed to the model as current); a false "invalidate" only costs one
extra real Read, same as today.

- Any `Edit`/`Write`/`NotebookEdit` `tool_use` in the session targeting the
  same `file_path` invalidates every indexed hash for that path immediately
  (evict, don't reissue — the next Read for that path is real and re-indexes).
- Any `Bash` `tool_use` whose `command` string contains the path as a
  substring invalidates that path. Deliberately coarse: `mv`/`rm`/`sed -i`/
  `git checkout -- <path>` or even an unrelated `echo <path>` all trip it. An
  over-invalidation costs one wasted Read; an under-invalidation is silently
  stale content in the model's context — that asymmetry justifies substring
  matching over a command-parser allowlist.
- mtime probe (optional, not required for v1): if the proxy can stat the file
  on the host filesystem (only true when proxy and Claude Code share one —
  not guaranteed for remote/containerized deployments), a newer mtime
  invalidates regardless of Edit/Bash matches. Deferred — it adds a
  filesystem-access dependency the pipeline doesn't otherwise have.

### Classes covered

| Class | Covered? | Why |
|---|---|---|
| Post-Edit re-read of the same file | **No — invalidated** | Edit invalidation (above) fires; the re-read is real, passes through unchanged, and re-indexes |
| `cat`/`cat -n <path>` via Bash | **Yes** | Matcher parses common `cat`/`cat -n` shapes from `command`; hashes the result the same as a Read |
| Offset-window subsumption (full read, then a narrower Read of the same unchanged region) | **Yes** | Index stores the full-file hash *and* line-range coverage; a Read whose `[offset, offset+limit)` falls inside a prior full or wider covering read on the same unvalidated hash is a hit even though the raw bytes differ (fewer lines) |
| Parallel-lane / subagent duplicate reads of the same file | **Yes** | Index is session_id-scoped, not lane/turn-scoped — every lane's outgoing body passes through the same proxy pipeline instance, so lane A's Read result indexes the hash lane B's identical Read then hits |
| Cross-session duplicate reads (same file, different session) | **No, out of scope** | Deliberately not shared cross-session — a stub pointing to a `tool_use_id` from a different session's history is not resolvable by the model reading it; would need a session-agnostic content-addressed store, a materially bigger design |

### Replacement stub text

Short, model-legible, and explicit that content was elided by policy (not lost
data — Layer 1 capture rule requires this framing to hold, see Safety below):

```
[proxy: duplicate read suppressed — file_path unchanged since tool_use_id
<prior_id> at <prior_timestamp>, which already returned this content in this
session. Refer to that turn's result. If you need to re-verify, re-issue with
force=true or after editing the file.]
```

The stub must carry the prior `tool_use_id` (resolvable via existing MCP
`get_request_tool_calls`) so the model — or an operator inspecting the
session — can trace back to the original bytes without re-fetching them.

### Safety

**Capture is unaffected.** `stored_body` (handler.rs ~line 308) snapshots the
original inbound request bytes *before* the filter/lazy-section/recall/steerer
mutation chain runs — dedup rewriting sits in that same post-capture chain, so
`.claude/rules/inspection-fidelity.md` Layer 1 is unchanged: the DB still holds
the full, real tool_result bytes the client sent. Safety by construction, not
added guarding — the pipeline already runs strictly downstream of capture for
every existing rewrite (filter rules, lazy sections, recall injection); dedup
is one more rewrite in that chain.

**The toggle.** Per `.claude/rules/no-default-off.md`, a feature toggle must
default ON and be `/settings`-settable. Read-dedup rewriting is a genuine
toggle (it changes what the model sees, with real correctness risk) — not a
structural-safety guard like `[compress] enabled=false`, so no carve-out
applies. Honest implication: **if this ships, it ships default-ON**, same as
filter rules, recall injection, and weasel intercept today — a real risk
surface (a stale-invalidation bug would silently feed a wrong stub to every
attached session, by default, on upgrade), which is exactly why this pass
proposes **shadow mode first** (below) rather than shipping the rewrite live.
Shadow mode is a distinct, non-mutating build phase preceding the mutating
toggle's existence, not a disabled toggle — once the mutating toggle exists it
must default ON; there is no rule-compliant "default-off to be safe."

### Failure modes

- **Stale stub after an out-of-band file change** the invalidation misses (e.g. a co-editor or second Claude Code instance on the same worktree edits the file without going through *this* session's Bash/Edit stream — the substring invalidation only sees this session's outgoing bodies). Mitigated by the mtime probe where filesystem access is available; otherwise accepted residual risk, same class multi-agent/multi-terminal editing carries today.
- **Model confusion from the stub** — a model that doesn't parse the stub may hallucinate content instead of looking up the prior turn. Mitigated by a short, imperative, consistently-worded stub.
- **Token accounting drift** — Anthropic's token counts and local estimators both measure the post-rewrite body; the saving is real in what's billed, but a dashboard assuming request body == what-the-user-typed will be wrong for these turns. Needs its own `content_bytes_saved`/`dedup_hit` counters, not silent.

## Alternatives considered

- **Steerer reminder instead of rewrite.** Inject a `<system-reminder>` (the
  steerer channel already does this) telling the model "you already read this
  file at turn N" instead of touching the tool_result body. Cheaper to build,
  zero stub-parsing risk — but doesn't save bytes: the duplicate result
  already crossed the wire by the time a reminder could react. Prevents the
  *next* duplicate, not the one just made — a companion, not a substitute.
- **Harness feature request only.** Ask for `Read` de-duplication upstream in
  Claude Code itself. Correct long-term home for the harness-native class
  (this doc's correction), but the proxy is the only lever this repo controls
  today, and multi-lane/subagent dedup may never be harness-visible even if
  built (each subagent is its own harness context — the proxy is the only
  vantage point that sees all lanes on one wire). Not mutually exclusive with
  pursuing this upstream.

## Rollout

**Shadow mode first, no rewrite.** Ship the detection index and a
would-have-deduped counter (bytes, hit count, per-session/per-path) with **no
body mutation at all** — pure observation, log-only:

- A new `/settings` stat, e.g. "Read-dedup: would save N KB / M% of Read bytes
  this session" — visible without any toggle, since nothing is mutating yet.
- `get_stats`/`diagnostic_stats` MCP surface gains the same counters, closing
  the loop the report's own methodology used (measure before automating).

Only once shadow-mode numbers are reviewed against real traffic (validating
the hit-rate and invalidation-false-negative assumptions above) does a
follow-up proposal build the actual rewrite, default ON, with its own
`/settings` visibility (the mutating behavior is not a togglable "safety"
feature per `no-default-off.md` — once built, it runs).

## Open questions

1. Does the harness send enough prior-turn history per request for the
   per-session index to stay warm without a cold-cache re-derive, or does the
   proxy need its own DB-backed index surviving process restarts?
2. Should the stub consume a `tool_result` slot, or drop to a minimal
   `is_error: false` result with the stub as `content` — does either shape
   risk the model treating a suppressed read as a tool error and retrying?
3. Where's the line between "duplicate" and "legitimately re-verifying" —
   should the stub honor an explicit forced re-Read by letting the *next*
   identical Read through unstubbed once, to avoid a stub-loop?
4. Does offset-window subsumption need real coverage-interval tracking, or is
   "any offset-window Read where a full-file hash exists and is unvalidated"
   sufficient — the report's cited cases (§1, §3) are dominated by full-file
   duplicates, so v1 may not need interval math at all.
5. Cross-lane dedup assumes all lanes share one proxy-side `session_id`. Is
   that true for every multi-lane pattern in the report (a519d811's 6 review
   lanes, ebe453d6's 12 parallel reviewers), or do some dispatch through
   separate sessions the proxy sees as distinct `session_id`s — in which case
   this design's "Yes" for cross-lane coverage needs re-verifying against the
   actual partitioning those sessions used.
