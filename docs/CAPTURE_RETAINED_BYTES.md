---
title: Capture retained bytes
status: active
created: 2026-11-01
updated: 2026-11-01
summary: Where one streamed response's bytes are held between the upstream read and the database, what bounds each stage, who owns it, when it is released, what happens under overload, and which of it the store's byte budget counts.
---

# Capture retained bytes

One path: an incoming response chunk, through the client/parser tee, to parsed events, a
persistence attempt, and the work retained for the terminal retry (temet-ai/mike-private#624). The plain tee is
`crates/mike-base/src/sse/tee.rs`; the gated forwarder (`weasel_intercept`) uses the same two
channels and capacities.

Logical bytes below are payload lengths. They are not allocator figures: a shared `Bytes`
counts once however many stages hold it, and RSS also carries allocator overhead and freed
pages not yet returned. The whole-process ceiling is `memory_cap` (RSS watermarks and capture
shedding), not anything in this table.

## Stages

| # | Stage | Holds | Bound | Storage | Owner | Released when | Overload |
|---|---|---|---|---|---|---|---|
| 1 | Client channel | `Result<Bytes>` chunks for the client body | 32 slots | shares the upstream chunk | forwarder task | the client reads it, or the client disconnects (forwarder stops) | forwarder waits on the client; upstream reads stop |
| 2 | Parser channel | `Bytes` chunks for the SSE parser | 64 slots | `chunk.clone()`: the same allocation as stage 1 | forwarder task | the parser reads it, or the parser task ends (forwarding continues alone) | forwarder waits on the parser, so a stalled parser also stalls the client |
| 3 | SSE event assembly | the current event's unfinished lines | one event | copied into the parser's buffer | parser task | the event's blank line | none beyond the event; Anthropic events are one JSON object each |
| 4 | Live broadcast | `ProxyEvent` clones for the inspect UI | 256 events, shared | owned clone per event | `AppState.event_tx` | each subscriber reads, or the ring overwrites it | slow subscribers lag and skip; capture is unaffected |
| 5 | Store channel | `ProxyEvent` (deltas, block starts, stop) | 512 events | owned strings per event | parser → store updater | the updater handles the event | `try_send` full: counted (`store_event_backpressured_total`), a debounced `memory.shed` notice, then the parser waits |
| 6 | Block accumulator | the open content block's text and signature | one block | owned `String` | store updater | the block is flushed to `responses.stream_blocks` (moved out with `mem::take`) | grows with the block; a block is what the model sent |
| 7 | Failed block buffer | `FailedStreamBlock` copies of blocks whose insert failed | every failed block of the request | owned copy (content, signature, ids, sha) | the request's store entry | the terminal snapshot succeeds (`remove_entry`), or the entry is evicted | counted in the store's byte budget (since temet-ai/mike-private#624); evicting a terminal entry with failed blocks drops them and is counted as an eviction |
| 8 | Terminal snapshot | a clone of the request for the terminal write | one request, transient | owned clone | snapshot task | the write returns | a failed write keeps the entry for the next terminal event |

## What bounds what

- Stages 1 and 2 bound slots, not bytes. A parked parser holds exactly 64 chunks, so its
  bytes scale with chunk size: 64 B at 1 B chunks, 64 KiB at 1 KiB, 4 MiB at 64 KiB
  (`sse/tee_retained_bytes_tests.rs`, synthetic chunks). The parser's chunk is the client's
  allocation, so the two queues together never hold a byte twice.
- Stages 3 to 6 are one event or one block deep, plus 512 parsed events of backlog.
- Stage 7 is the only buffer that grows with persistence failure. During a database outage
  every block of every streaming request lands here. Before temet-ai/mike-private#624 the store's byte budget
  (`max_store_bytes`, default 3 GiB) and the cap manager's shed estimate did not count it;
  `store/eviction.rs` `entry_bytes` now does, so the budget evicts and sheds against the
  real retained bytes, and `/api/diagnostics/memory` reports them as
  `failed_stream_blocks_bytes` inside `entries_total_bytes`.

## Release exactly once

| Ending | Stage 1-2 | Stage 6 | Stage 7 |
|---|---|---|---|
| response completes | drained by readers | flushed | released by the terminal snapshot's `remove_entry` |
| client disconnects | forwarder stops; channels drop | updater sees the stream end and flushes | as completion |
| parser task ends | forwarding continues client-only; parser channel dropped | updater ends with the events it had | as completion |
| database down until eviction | as completion | insert fails, block moves to stage 7 | dropped with the entry, counted as a store eviction |

A dropped failed block is captured content that never reached the database. It is visible
as a store eviction and in the request's missing `stream_blocks` rows; the capture does not
claim a complete durable record for it.
