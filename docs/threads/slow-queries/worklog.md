---
thread: slow-queries
status: done
last_session: 2026-06-09 (session unknown) @ 8497a43
---

## State of play
Where we are: Active monitoring. Postgres `log_min_duration_statement = 100ms`. Real-time monitor on docker container logs + 15-min cron sweep (job `be404be4`) running `scripts/slow-query-report.sh --since 15m --append`.
Next: (1) Record and analyze each new slow query as it arrives.
Open questions: none.
Decisions: For each slow query — duration, EXPLAIN ANALYZE, table indexes, codebase call site, verdict. No fixes in this thread.
Blockers: none.

## Analysis script

```bash
# Manual run — last 15 min, print only:
./scripts/slow-query-report.sh

# Custom window:
./scripts/slow-query-report.sh --since 1h

# Print + append to this worklog:
./scripts/slow-query-report.sh --since 24h --append
```

## Session log
- 2026-06-09 (unknown) @ 8497a43 — thread created. Monitoring proxy logs for slow query patterns.
- 2026-06-26 (cb8398c9) @ 419a466a — Closed (status: done, operator-directed). This was a standing-monitor thread, not a deliverable; closing it retires the monitor. The 15-min sweep cron (job `be404be4`) was session-scoped to the now-ended session that created it, so it is no longer running; `scripts/slow-query-report.sh` remains available for ad-hoc sweeps. Restart a monitor under a fresh thread if continuous slow-query watching is wanted again.

## Observed slow queries

<!-- Append entries here as slow queries are detected -->

### 2026-06-09 06:32:26 UTC — 147ms DDL (startup migration)
```
duration: 147.349 ms  statement: CREATE SCHEMA IF NOT EXISTS claude_proxy;
```
**Context:** Fired once at proxy startup during migration run. DDL on schema creation — expected to be slow on first run or after a restart. Not a recurring application query.
