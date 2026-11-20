---
title: Compression Toolset — Complete Capability Map and Delivery Plan
status: implementation-ready
created: 2026-07-20
updated: 2026-07-20
summary: A code-grounded plan for the complete compression system, with paired original/sent capture, an explainable transformation manifest, and an immediately measurable first slice.
---

# Compression Toolset

## Decision

Build the full compression system incrementally on the existing `mike` request
pipeline. Start with one concrete, offline file-read reducer and a safe raw-byte
replacement primitive. Use its replay evidence to decide the live policy, then add
the remaining content types in measured value order.

This replaces the earlier seven-stage design. That design mixed four different
concerns into one proposed change: compression algorithms, an experiment platform,
a new control plane, and speculative persistence/UI. It was simultaneously too
large to start and incomplete as an end-state map: it omitted several capabilities
identified in the Headroom research, including source-code, search, diff, browser
text, shared-context, and learning paths.

This document separates those concerns:

- **Complete destination:** every capability required for a fully featured system
  remains visible in the inventory and completion checklist.
- **Immediate delivery:** only the smallest safe, high-value slice is in the first
  implementation packet.
- **Evidence before live mutation:** modeled opportunity chooses the order; corpus
  replay chooses the actual policy and whether a capability ships.

## Evidence-led priority

The week-long analysis measured 5,797 million prompt-side tokens across 332
sessions. The totals below are derived from the modeled class ratios in
`artifacts/2026-07-20/compression-analysis/savings-model.json`; they are opportunity
estimates, not achieved savings.

| Target                 | Modeled central opportunity | Why it is ordered here                                                                 |
| ---------------------- | --------------------------: | -------------------------------------------------------------------------------------- |
| File reads             |      about 460M tokens/week | Largest modeled class and can be reduced with a deterministic, re-readable text policy |
| Browser screenshots    |      about 345M tokens/week | Second-largest; the model assumes stale-screenshot eviction, not image downscaling     |
| Shell/build/log output |      about 263M tokens/week | Large, text-shaped, and suitable for deterministic error/head/tail retention           |
| MCP JSON               |      about 132M tokens/week | Valuable, but shapes vary and lossy array/object policies need retrieval               |

Two findings change the old phase order:

- Exact duplicate tool results were only 0.3–1.5% of tool-result characters in the
  sampled contexts. Read dedup remains useful, but it is not an early savings target.
- A live-store check on 2026-07-20 found the existing `JsonMinify` plus
  `WhitespaceSquash` spike saved an estimated 0.43% across the nine requests then in
  memory. That is a small live snapshot, not corpus proof, but it is enough to reject
  the existing generic compressor chain as the main roadmap.

The source analysis and the limits of the model remain in
`artifacts/2026-07-20/compression-analysis/report.md`. Every future claim must keep
modeled, replay-measured, and live-observed results separate.

## Non-negotiable safety constraints

1. **Capture is a paired audit record, not an original-only archive.** Every live
   mutation keeps the exact client body and the exact final body handed to the
   upstream request builder beside each other under the same request id. The
   original body, nested image data, tool results, and signatures remain fully
   stored; the sent body contains the compressed representation plus every later
   filter, injection, and composition change. Both are first-class inspection
   bodies, not a full body plus a truncated preview.
2. **No whole-body reserialization on the live path.** The current measurement
   spike parses into `serde_json::Value` and serializes the whole body in
   `compress/content_blocks.rs`. That is acceptable for a hypothetical diff, not
   for forwarding: it changes unrelated whitespace, key order, and escape choices.
3. **Replace exact byte ranges and fail closed.** Live compression follows the
   composer's `compose_prompt::compose_template` precedent: identify the exact raw
   JSON value being replaced, copy every byte before and after it verbatim, and
   return the input unchanged when the range is missing or ambiguous.
4. **Signed thinking content is sacrosanct.** No compressor may deserialize and
   re-emit a surrounding message containing `thinking`, `redacted_thinking`, a
   signature, or encrypted content. Tests compare those byte ranges directly.
5. **Only shrinking rewrites are accepted.** A candidate that does not reduce the
   chosen token count, or at minimum the encoded byte length before tokenizer
   integration, is discarded.
6. **Determinism is the cache rule.** Claude Code resends full history; whole request
   bodies are not append-only byte prefixes. Repeated input plus frozen policy must
   produce identical replacement ranges and bytes on every request.
7. **No hidden hot-path work.** Do not hold the store lock while classifying or
   transforming content. CPU-heavy image or model work must be bounded and moved
   off Tokio worker threads. Collections and recovery stores must have explicit
   capacity and retention.
8. **Every delta is attributable.** Store an ordered transformation manifest beside
   the original/sent body pair. It records which mutation stages were evaluated,
   what each stage targeted, the resolved policy and parameters, whether it applied
   or failed closed, and its input/output hashes and measurements. A body diff shows
   _what_ changed; the manifest explains _which code path changed it and why_.
9. **Inspection records the real final send.** `filtered_request_body` is currently
   filled from `filtered_stored`, while the composer can later create a different
   `upstream_body`. The existing `/body/request_sent` name therefore overstates its
   guarantee for composed requests. Live compression is blocked until the stored
   sent body comes from the final `upstream_body` immediately before send and its
   hash closes the transformation manifest.
10. **No response compression.** The response body, SSE parser, event ordering,
    content type, rate-limit headers, and upstream `accept-encoding: identity`
    contract are out of scope.
11. **No framework before implementations.** The existing `Compressor` trait models
    context-free mutation of one `Value`; producer-aware reducers need paired
    `tool_use` data and raw ranges. Add concrete functions per capability. Extract a
    shared abstraction only after at least two implementations prove the same seam.

## Paired capture and explainability contract

The inspect model for a mutated request has three adjacent parts:

1. **Original body:** byte-for-byte `request_body` received from the client.
2. **Sent body:** byte-for-byte `request_sent_body` handed to `reqwest` after the
   entire request mutation chain. The existing nullable `filtered_request_body`
   column may remain the physical storage optimization initially: when the final
   bytes equal the original, `/body/request_sent` returns the original; when they
   differ, the column must contain the final `upstream_body`, not an earlier
   `filtered_stored` snapshot.
3. **Transformation manifest:** the full ordered explanation of how the first body
   became the second.

The manifest covers the complete mutation chain—filters, reminder/injection stages,
compression, and composition—because a compression-only ledger cannot explain the
final body. Use one compact, versioned manifest stored with the request before
normalizing it into tables. Normalization is justified only when a demonstrated
query cannot be answered from the captured manifest.

```text
TransformationManifest
  schema_version
  original_sha256
  sent_sha256
  stages[]
    ordinal
    stage                         # e.g. filter, reminder, compression.file_read, composer
    outcome                       # applied | unchanged | skipped | failed_closed
    policy_id + policy_version
    resolved_parameters           # actual frozen values, not just current settings
    input_sha256 + output_sha256
    targets[]
      target identity             # JSON path, tool_use_id, producer, content kind
      input range + output range  # offsets in this stage's input/output bytes
      original/sent byte and token counts
      original/sent content hashes
      recovery reference, when lossy
      stable reason code + detail
```

The hashes form a verifiable chain: a stage's `input_sha256` equals the previous
stage's `output_sha256`, the first input equals `original_sha256`, and the last
output equals `sent_sha256`. Record one stage entry for each mutation-capable stage
that was evaluated; record target entries only for detected candidates. This makes
an applied rewrite, an intentional no-op, and a fail-closed decision distinguishable
without recording another full body for every intermediate stage.

Capture and forwarding are one contract for live compression. If mike cannot reserve
capacity for the original body, final sent body, and manifest, compression returns
its input unchanged. When a request row can still be recorded, its manifest carries
`skipped: capture_unavailable`. Mike must never send a compressed body that has no
paired original and manifest.

Inspection exposes the three parts together:

- REST keeps the existing full `/body/request` and `/body/request_sent` siblings and
  adds the full manifest to the request-body index/detail surface;
- MCP keeps `get_request_body` and `get_request_sent_body` and gains manifest access
  through route-manifest parity;
- the request detail UI renders Original, Sent, and Applied Transformations together,
  with an optional full diff between the two bodies;
- any lean stage summary has the full manifest and both full bodies at the same call
  site, as required by `.claude/rules/inspection-fidelity.md`.

## Scope boundary

`mike` is already the Anthropic reverse proxy, so this plan does not add wrapper,
SDK, multi-provider, OpenAI, Gemini, Bedrock, or Vertex deployment shapes. Those are
separate provider-adapter projects. This plan covers Anthropic `/v1/messages`
request context and the existing mike inspect/MCP/control surfaces.

## Complete capability inventory

The table is the product map. “Later” does not mean optional; it means the
dependency or value gate has not yet been cleared.

### Shared foundations

| Capability                      | Required behavior                                                                                                                      | Existing seam                                                                                    | Delivery point                                                                  |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| Raw request rewriter            | Locate target raw JSON ranges, apply non-overlapping replacements, preserve all other bytes, fail closed                               | Composer byte-splice pattern                                                                     | First offline slice; live-harden before any live reducer                        |
| Producer/content classification | Pair `tool_result.tool_use_id` with its `tool_use` and classify actual nested content, including images inside `tool_result.content[]` | Pairing logic in `read_dedup.rs` and request extractors                                          | Add only the classifications needed by each reducer; consolidate after two uses |
| Token/no-shrink gate            | Compare original and candidate using provider token counts or a calibrated class estimator; retain original on no win                  | `compress/measure.rs`                                                                            | Byte gate first; token gate before live lossy policies                          |
| Paired original/sent capture    | Keep exact client bytes and exact final upstream bytes adjacent under one request id; derive modified status from byte inequality      | `request_body`, `filtered_request_body`, and existing request/request-sent REST + MCP body tools | Fix final-body write semantics before the first live reducer                    |
| Transformation manifest         | Persist the ordered mutation chain with policy/version/config, target identity and ranges, hashes, measurements, recovery, and reason  | Existing preview stats, filter rule ids, composer observation, and tracing                       | Return from preview first; store beside the body pair for every live mutation   |
| Cache drift observation         | Detect whether bytes outside declared replacement ranges changed and report the first differing range                                  | Existing composer/cache-scope tests                                                              | Test helper first; live metric before broad rollout                             |
| Reversible originals            | Keep complete originals in capture and provide an unambiguous way to recover elided content                                            | Request/tool-result capture and mike MCP                                                         | Class-specific recovery first; shared retrieval before generic lossy reducers   |

### Input-context reducers

| Capability                   | Finished behavior                                                                                                               | Dependency and order                                                                                                                  |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| File reads                   | Reduce large `Read` results deterministically; preserve useful boundaries and emit exact path/range instructions for re-read    | **First.** Start with text policy; add AST-aware source skeletons only after replay shows language mix and value                      |
| Source code                  | Preserve imports, signatures, types, declarations, error handling, and selected bodies using language-aware parsing             | Follows file-read baseline; bounded parser dependencies and syntax-validity fixtures required                                         |
| Stale screenshots            | Keep the actionable recent screenshot set and replace older nested image parts with deterministic recoverable references        | **Second.** Real captures prove images are nested; policy must be derived from message order, not a top-level `image` match           |
| Image downscaling            | Optionally resize oversized current screenshots with fixed, safe encoder settings                                               | Separate experiment. The current 345M model does not estimate downscaling, so it does not inherit the eviction business case          |
| Shell/build/log output       | Preserve failures, warnings, command summary, head, and tail; elide repetitive middle regions                                   | **Third.** Replay per producer because Bash, TaskOutput, test, and build formats differ                                               |
| MCP JSON                     | Use lossless minification first; for lossy arrays/objects preserve schema, errors, first/last items, anomalies, and recovery id | **Fourth.** Shared retrieval is required before lossy mode                                                                            |
| Search results               | Preserve query, source identity, ranking, errors, and representative results while dropping repetition                          | After the four measured classes; needs producer fixtures                                                                              |
| Git diffs                    | Preserve file headers, hunk headers, changed lines, and error context; reduce unchanged/repeated context                        | After search; exact patch semantics and syntax fixtures required                                                                      |
| Browser/web/HTML text        | Extract meaningful page text, headings, links, form state, errors, and relevant DOM without base64/image payloads               | After screenshot policy; do not conflate page text with images                                                                        |
| Plain text                   | Apply a deterministic extractive policy only where typed reducers do not apply                                                  | Late. A learned/ONNX compressor is justified only if replay finds substantial residual mass and its latency/memory are bounded        |
| Exact duplicate suppression  | Replace safe repeated outputs with a stable reference and invalidate on mutations                                               | Later because measured duplication is small; reuse read-dedup findings but do not make live correctness depend on its in-memory index |
| Replayed tool-call arguments | Reduce historical arguments only where they are no longer executable and correlation remains intact                             | Late, separately measured; never mutate the current tool call sent for execution                                                      |

### Output, sharing, and learning

| Capability                | Finished behavior                                                                                                                   | Boundary                                                                           |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Output verbosity          | Stable initial verbosity instruction plus conservative session feedback that backs off immediately when detail is requested         | Separate output-side track; do not hide it inside input compression                |
| Effort routing            | Lower effort/verbosity only where policy permits; never silently raise or alter unsupported model fields                            | Requires model-specific tests and live quality evidence                            |
| Shared compressed context | Store bounded, keyed compressed handoff context for collaborating agents with access to the full original                           | Application/MCP layer after core reducers; not request-capture storage             |
| Feedback and learning     | Record re-read, re-fetch, retrieval, interruption, and failure signals; use them to tune policy or produce reviewed recommendations | Observe first. No self-modifying live policy without an operator-reviewed artifact |

### Operator and interface coverage

Each reducer that reaches the live path eventually needs all of the following:

- one default-on, session-scoped feature toggle, added only when that reducer is
  safe to ship, using the existing generic `/settings` feature catalogue;
- settings only for parameters replay proves need operator control—no speculative
  eleven-setting matrix and no dedicated page before the generic catalogue is
  insufficient;
- preview output with the full original body, full candidate sent body, ordered
  manifest, full diff, and per-target measurement;
- global/session stats backed by the smallest honest data source; apart from the
  minimal storage needed for the paired sent body and manifest, add no statistics
  schema until a live question cannot be answered from existing capture and token
  rows;
- adjacent Original, Sent, and Applied Transformations inspection in the request
  detail UI, backed by the full REST bodies and manifest;
- REST route-manifest coverage and an MCP tool for the transformation manifest—the
  existing `get_request_body` and `get_request_sent_body` tools remain the full-body
  siblings;
- full-content retrieval beside any lean summary or recovery reference, in line
  with `.claude/rules/inspection-fidelity.md`;
- UI rendering only when there is an operator workflow to render, verified against
  the live system rather than satisfied by a settings-card mockup.

## Ordered delivery plan

### 0. Immediate: offline file-read reducer

Implement one concrete file-read policy behind the existing
`POST /api/compress/preview`. It must pair `Read` tool uses with string-shaped
results, leave small/error/unknown shapes untouched, produce a deterministic
candidate with explicit re-read instructions, and replace only the encoded result
content range in the original bytes.

There is no hot-path call, feature toggle, migration, table, enrolment service,
settings group, new route family, MCP tool, or frontend page in this slice.

Replay representative large file-read requests first, then the captured corpus.
The output is a local report containing coverage, actual byte/token reduction,
no-op reasons, policy failures, language mix, and examples selected for semantic
review. The replay numbers replace the 50%/35% model assumptions; they do not merely
confirm them.

For each previewed request, return the full original body, the full candidate sent
body, and the same ordered manifest shape intended for live capture. The file-read
stage is the only stage in this offline manifest, but it still records unchanged,
skipped, and failed-closed outcomes so its contract can be reviewed before storage
work begins.

**Exit:** safe-splice tests pass, the report exists, and the operator can decide
whether the measured reduction and retained content justify live work.

### 1. Live file-read vertical slice

Only after Phase 0 evidence:

- wire the exact replayed function between read-dedup observation and the composer;
- keep read-dedup separate—its stateful newest-pair observation is not the live
  compression engine and sharing one parse is not a reason to merge responsibilities;
- add one session-scoped, default-on file-read toggle to the existing generic
  settings surface;
- replace the current `filtered_stored` audit snapshot with an exact final-body write
  from `upstream_body` immediately before the upstream request is built; compare it
  byte-for-byte with `request_body` rather than trusting mutation flags;
- accumulate and durably store the ordered transformation manifest for every
  mutation-capable stage, then verify its hash chain closes on the two stored bodies;
- expose Original, Sent, and Applied Transformations together through REST, MCP, and
  request detail inspection without truncation;
- observe at least one real new session end to end, including successful Anthropic
  responses, stable repeated-history bytes, retrieval/re-read behavior, and token
  counters.

**Exit:** local gates are clean and a live session proves both savings and quality.

### 2. Stale screenshot policy

Classify real browser producers, walk nested `tool_result.content[]` image parts,
and replay a deterministic “keep recent actionable images” policy. Add downscaling
only as a separate candidate with its own measured result. Do not add an image codec
to the hot path on the strength of the eviction estimate.

**Exit:** corpus reduction, screenshot re-fetch rate, detail-loss review, CPU/memory
bounds, and recovery behavior are known before live wiring.

### 3. Logs, then MCP JSON

Build and replay one concrete log reducer, then one JSON reducer. Reuse the raw
replacement seam only after the file-read and screenshot implementations prove the
shared shape. Introduce shared retrieval before any JSON policy drops information
that cannot be recreated by repeating the original tool call.

**Exit:** each reducer has its own fixtures, no-shrink/token gate, recovery path,
toggle, live evidence, and rollback control.

### 4. Remaining deterministic content types

Add search, diff, browser/HTML text, and then language-aware source-code policies in
measured residual-value order. Exact duplicate suppression follows only if live
measurement exceeds the existing low duplicate rate.

### 5. Recovery and product surface

Generalize class-specific recovery into a bounded retrieval store/tool only after
two reducers need it. Then add durable statistics, dedicated UI, or new schema only
for demonstrated operator questions. Keep tool definitions stable for a session so
retrieval cannot churn the prompt-cache prefix.

### 6. Output reduction, shared context, and learning

Deliver these as separate tracks using the proven reducers and metrics. Output
verbosity/effort affects generated responses; shared context affects handoffs;
learning affects future policy. None belongs in the raw request-rewrite function.

## First implementation packet

### Objective

Produce measured, reviewable file-read reduction without changing live traffic.

### Files and symbols

- `crates/mike/src/compress/file_reads.rs` — new concrete pairing, reduction, and
  fail-closed raw replacement logic. Keep any byte-range helper private here until a
  second reducer proves it is shared.
- `crates/mike/src/compress/mod.rs` — expose the concrete preview outcome; do not add
  a stage trait, registry, mode enum, or generic dispatcher.
- `crates/mike/src/api/handlers.rs` — make the existing `compress_preview` run the
  file-read candidate and return both full bodies, the ordered manifest, its full
  diff, and measurements.
- colocated unit tests in `compress/file_reads.rs`; add no proxy integration test
  until there is a proxy call site.
- `artifacts/YYYY-MM-DD/compression-file-read-replay/` — local replay results and
  reviewed examples; this is evidence, not production documentation.

### Policy requirements

- Match only paired `Read` results in known string/text shapes.
- Skip error results, small reads, malformed pairs, and already-reduced markers.
- Preserve a useful beginning and ending plus a marker carrying file path, original
  line count, retained ranges, omitted range, and exact re-read guidance.
- Use named constants selected by replay; do not pre-register settings for guesses.
- Return unchanged on parse, encode, range-location, ambiguity, or no-shrink failure.

### Do not touch

- `proxy/handler.rs`, `read_dedup.rs`, `store/`, migrations, feature toggles,
  settings, frontend, `mike-mcp`, response forwarding, and `sse/`;
- system prompts, `tools[]`, assistant thinking blocks, current executable tool
  calls, and captured originals;
- the unrelated dirty worktree files.

### Verification

Tests must prove:

- a no-candidate path returns the exact input bytes;
- every byte before and after a replacement range is unchanged;
- signed thinking blocks elsewhere in the request remain byte-identical;
- deterministic replay and idempotence;
- no-shrink fallback;
- malformed, ambiguous, unpaired, small, and error inputs pass through;
- multiple `Read` results in one turn are paired by `tool_use_id`, not position;
- preview returns the full diff and honest per-result totals;
- preview returns byte-identical full original and candidate-sent bodies beside a
  manifest whose hash chain closes on those bodies;
- every applied, unchanged, skipped, and failed-closed decision has a stable reason
  and the resolved policy values used for that decision.

Implementation gates:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --all
scripts/fidelity-check
```

## Definition of fully featured

The compression toolset is not complete until all applicable items are either
shipped with evidence or explicitly rejected with a measured reason:

- [ ] Safe raw-range rewrite engine and cache-drift verification
- [ ] Producer/content classification for real Anthropic tool-result shapes
- [ ] Token-aware no-shrink gate
- [ ] Paired full original and exact final-sent body capture under one request id
- [ ] Ordered transformation manifest whose hash chain closes on the captured pair
- [ ] File-read reducer and language-aware source-code follow-up
- [ ] Stale-screenshot policy and separately evaluated image downscaling
- [ ] Shell/build/log reducer
- [ ] MCP JSON reducer
- [ ] Search-results reducer
- [ ] Git-diff reducer
- [ ] Browser/web/HTML text reducer
- [ ] Residual plain-text decision, including learned compression only if justified
- [ ] Exact duplicate and historical tool-argument decisions
- [ ] Full-original recovery and bounded shared retrieval
- [ ] Cache stability/drift metrics
- [ ] Output verbosity and effort track
- [ ] Shared compressed context for agent handoffs
- [ ] Feedback/learning loop with operator review
- [ ] Per-capability settings/toggles based on measured needs
- [ ] Honest durable stats where existing capture is insufficient
- [ ] Adjacent Original, Sent, and Applied Transformations UI inspection
- [ ] Full REST and MCP access to both bodies and the manifest, with route parity
- [ ] Inspection-fidelity coverage for original, sent, manifest, diff, and recovery
- [ ] Corpus replay, targeted tests, all workspace gates, and live-session evidence

## References

- `artifacts/2026-07-20/compression-analysis/report.md` — corpus composition,
  modeled savings, duplicate rate, and limits
- `artifacts/2026-07-20/compression-analysis/savings-model.json` — class weights and
  central/conservative assumptions used for the priority table
- `artifacts/2026-07-20/compression-analysis/headroom-research.md` — complete
  reference capability survey, including content routing, retrieval, live-zone,
  output shaping, shared context, and learning
- `crates/mike/src/compress/` — current measurement-only spike
- `crates/mike/src/compose_prompt/mod.rs` — existing fail-closed byte-splice pattern
- `crates/mike/src/read_dedup.rs` — current shadow-only Read pairing and duplicate
  measurement
- `crates/mike/src/proxy/handler.rs` — capture, read-dedup, composer, audit, and send
  ordering
- `.claude/rules/inspection-fidelity.md` — full-content capture/projection contract
- `.claude/rules/no-default-off.md` — live feature-toggle contract
