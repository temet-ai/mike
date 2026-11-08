---
title: Capture outages
status: active
created: 2026-11-05
updated: 2026-11-05
summary: What mike does when the database is down or the disk is full, how the person is told, which captures are lost, and how a stop that could not finish says so.
---

# Capture outages

Operator decision, 2026-11-05 (temet-ai/mike-private#63): when the database is down or the disk is full, mike
keeps forwarding so Claude keeps working, and tells the person there is a problem. Nothing
is refused.

## Behaviour

| Condition | Forwarding | Capture | The person is told |
|---|---|---|---|
| Database unreachable | continues, bytes unchanged | writes fail; blocks of streaming requests wait in the failed-block buffer (stage 7 of [CAPTURE_RETAINED_BYTES](CAPTURE_RETAINED_BYTES.md)) | desktop notice and an `mike::operator_notice` error log after two failed readings (about two minutes) |
| Database answers but writes stall (disk full, read-only volume) | continues | not written | the same notice, from `capture_stalled` in `/api/health` |
| Database back | continues | writes resume | one recovery notice |

The notice fires once per outage, however long it lasts, and once more when captures are
saved again. It is produced by the health refresher
(`crates/mike/src/api/handlers/health/capture_outage.rs`), which reads the same
reachability and `capture_stalled` signals `/api/health` reports, so the page and the
notice agree. Delivery is the desktop notification the worker-failure notice uses
(`omarchy-notification-send`, then `notify-send`); with neither installed the error log is
the notice.

## Loss window

- Detection takes up to two refresher ticks, 60 seconds each, plus the probe deadline.
  Requests in that window are captured only if the database is back before the buffers below
  drop them.
- A request forwarded during the outage is not recorded unless its failed blocks are still
  in the buffer when the database returns. The buffer is bounded by `max_store_bytes`
  (default 3 GiB); past it the oldest entries are evicted and counted as store evictions.
- A request evicted or lost at stop has no durable record. Mike does not mark it as
  complete; its `stream_blocks` rows are missing.
- Nothing already stored is affected.

## Stopping

A stop closes the listeners, waits up to 10 seconds for connections, then up to 30 seconds
for background tasks (including capture writes) to finish.

- If the task wait runs out, mike logs `shutdown drain timed out after 30s with N background
  task(s) unfinished` at error level, does not log `shutdown complete`, and exits non-zero.
  Captures those tasks were still writing may be lost.
- If shutdown as a whole passes 60 seconds, the hard deadline logs that and exits with
  status 1.
- A clean stop logs `shutdown complete` and exits 0. Only a clean stop means every capture
  that was in flight reached the database.
