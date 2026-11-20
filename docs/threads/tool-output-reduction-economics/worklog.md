---
thread: tool-output-reduction-economics
status: done
last_session: 2026-06-15 (session 23214930-40a5-41bd-b773-3d13e30b10b8) @ 7bc4110b
---

## State of play

Where we are: Investigated whether captured tool-call data can justify building a
tool-output reduction feature (the rtk-style "this command cost X tokens; a leaner
equivalent saves Y" play). Measured the real token economics from `responses.responses`.
The cache reframes everything.

**Key finding — the cache makes re-sends cheap, so history-editing dedup is net-negative.**
Of all input-side tokens (18,409 responses with usage):

| Bucket | Raw tokens | Weight | Input-equiv | Share |
|---|---|---|---|---|
| cache_read | 2.65B | 0.1× | 265M | 57% |
| cache_create | 108M | 1.25× | 135M | 29% |
| output | 11.5M | 5× | 58M | 12% |
| fresh input | 5.1M | 1× | 5M | 1% |

- **95.9%** of input-side tokens are cache reads (0.1×). Fresh full-price input is **0.2%**.
- The earlier "646M wasted re-sent tokens" figure is **gross bytes**, mostly cache_read at
  0.1× — not recoverable full-price cost.
- **Editing committed history to dedup is net-negative**: removes 0.1× tokens, breaks the
  prefix, forces downstream re-cache at 1.25×. The cache exists precisely so you should not
  pay to remove re-sends.
- **Carry-forward multiplier** = cache_read / cache_create = 2.65B / 108M ≈ **24.5**. Each
  cached token is re-read ~24.5×; lifetime cost ≈ 1.25 + 24.5×0.1 ≈ **3.7× input-equiv**.
- **The only real lever is first-emission size** — shrink a tool result *before* it enters
  the cached prefix, saving the one-time 1.25× write + 0.1× on every re-read (where 24.5×
  compounds). Cache-safe because you shape content before it is committed.
- Wire position is structurally weaker than the agent (rtk) for this, with **one exception**:
  the proxy stripping an output **deterministically from its first appearance** lets the
  cache form on the smaller stream (one-time miss, then steady-state).

Reduction-target classes, re-ranked by the corrected economics:
1. **Class 2 — our own MCP list-tools** (we own the code; fresh content we fully control;
   leaner projections shrink before caching). Now top priority. list_session_tool_uses
   22 KB/call avg, list_requests 18.9 KB, list_sessions 17.6 KB, get_session 14.7 KB.
   `get_session_head` already proves the lean pattern. Fix = leaner default projection +
   pagination, NOT truncation (inspection-fidelity §2).
2. **Class 3 — agent shell commands** (suggestion-only; we are on the wire). grep fat tail:
   3% of 130,922 calls exceed 5 KB but hold ~8.3M tokens; 69 calls >20 KB.
3. **Class 1 — history dedup: KILLED** by the cache economics above. Keep only as
   first-emission deterministic-strip if pursued at all.
4. **Class 4 — browser_batch** 783 KB screenshots: not our code, niche. Skip.

Next:
(1) Re-measure per-command-family cost in **input-equivalent units with the 24.5×
    multiplier** so the ROADMAP figure is honest lifetime cost, not gross bytes.
(2) Decide build target: Class 2 (lean projections, we own it) vs Class 3 (suggestion play).
(3) If pursuing any wire-side output reduction, design the deterministic-strip-from-first-
    appearance mechanism and prove cache-neutrality.

Files:
- /Users/dawiddutoit/projects/temet/temet-proxy/docs/research/rtk-competitive-analysis.md — competitive analysis + transferable ideas
- /Users/dawiddutoit/projects/temet/temet-proxy/ROADMAP.md — Phase 20 (opportunity miner & savings economics)
- /Users/dawiddutoit/projects/temet/temet-proxy/.claude/rules/inspection-fidelity.md — the no-truncation constraint any reduction feature must respect
- Capture DB: `responses.responses` (input_tokens, cache_read_input_tokens, cache_creation_input_tokens, output_tokens), `tools.results` (content_bytes, content_sha256), `tools.uses_v` (command)

Open questions:
- Per-command-family lifetime cost in input-equivalent terms — not yet measured (Next #1).
- Is Class 2 projection bloat real (fields no consumer reads), or are the 14–22 KB rows
  all load-bearing? Needs a field-level audit before cutting.

Decisions:
- History-editing dedup is OFF the table — cache economics make it net-negative.
- Reduction, if built, must be first-emission and inspection-fidelity-safe (dedup/escape-
  hatch/lean-projection, never truncation).

Blockers: none.

## Session log
- 2026-06-15 (23214930-40a5-41bd-b773-3d13e30b10b8) @ 7bc4110b — thread created. Measured
  cache economics: 95.9% cache_read, 24.5× carry-forward multiplier. Concluded history-edit
  dedup is net-negative; real lever is first-emission size. Re-ranked reduction classes
  (Class 2 our-own-MCP now top, Class 1 history-dedup killed).
- 2026-06-26 (cb8398c9) @ 419a466a — Closed (status: done). Investigation concluded: decisions
  recorded (history-edit dedup is OFF the table; the only lever is first-emission, inspection-fidelity-safe
  reduction) and captured as ROADMAP Phase 20 (research). No code deliverable owed; the remaining "Next"
  items are optional deeper measurement, tracked under Phase 20 if ever pursued.
