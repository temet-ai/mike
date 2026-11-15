---
title: "#200 knowledge search speed: what is true on 2026-11-15, the five fixes compared"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: The issue's numbers hold on today's build (SQL 1.4-1.7 s, whole call 1.8-5.2 s); one unrelated plan fix landed 11-12, none of the five fixes has. Order 5, 2, 1, then 4; skip 3.
session_id:
  - ddf91a5c-37a8-4a22-a59f-c0fb00adb0a0
---

# #200 knowledge search speed

## What is true now (measured 2026-11-15, omit, read only)

- **SQL.** `EXPLAIN (ANALYZE, BUFFERS)` of `search.sql` (project 993478, query "fleet coordinator stop fault herdr", any-term mode, custom plan, three runs): execution 1405 / 1464 / 1724 ms, planning 28-47 ms. The `cheap` append still builds 18,232 full rows (issue: 17,604). The same query in all-terms mode: 55 ms execution. Issue number confirmed; the rows grew by 3.6% with the corpus.
- **Metrics since the proxy started** (`/api/metrics`): `db_query_duration_ms{method="knowledge_search"}` 1,909,795 ms over 1,450 calls = **1.32 s average** (issue: 2.1 s; the average now includes cheap narrow queries, so the heavy query is the 1.4-1.7 s above). `search_phase_duration_ms{phase="rerank_request"}` averages 251 ms over 1,306 calls. The `knowledge_search_handler` phase has 3 samples, so it cannot show the whole-call average; the done-when needs it fixed or a second source (see Measure).
- **Whole call.** `/api/knowledge/search` for the issue's query: **5.25 s** first call, **1.77 s** repeated. Issue: about 3.9 s average, 0.5 s on a hit; the repeat is slower than the issue's cache hit, so the SQL, not only the models, is on the repeat path.
- **Models.** Ollama now holds embeddinggemma (685 MB, 100% GPU, unloads in 5 minutes); no `keep_alive` or `OLLAMA_KEEP_ALIVE` anywhere in `crates/` or the Ollama units (only `OLLAMA_HOST`, `OLLAMA_MODELS`, `HOME`). The reranker proxy still runs `--exit-idle-time=600` (`mike-stack-rerank-proxy.service`); it started 29 times in 7 days (issue: 54). The GPU shows 2,005 of 8,192 MiB in use with the embedder loaded.
- **Has a fix landed since 11-13? No.** `git log --since=2026-11-13` on `crates/mike-knowledge/src/store/persistence/project_knowledge/` and `src/api/project_knowledge/` shows only the crate extraction (50a178fe2). `search.sql` is 639 lines, the same text. What did land is `d136371e6` (2026-11-12, in main): `.persistent(false)` at `search.rs:141`, which stops the generic plan that took 9.35 s. That is why the issue measured a 2 s search and not a 9 s one; it is not one of the five fixes. The `knowledge-sweep-idle` worktree listed on the roster is not in `git worktree list`, so no change to these files is in flight that I could see.

## The five fixes compared

| # | Fix | Saving (issue's timings; checked against the 1.4-1.7 s above) | Effort | Risk |
|---|---|---|---|---|
| 1 | Rank narrow rows first, build full rows for the ~650 survivors | 0.9-1.2 s of 1.4-1.7 s (narrow pass 490-515 ms against 1.26-1.74 s for `cheap`) | Largest: one CTE restructure across 12 lexical branches in a 639-line file, which is already over the 500-line limit, so a split is needed too | Medium: ordering for `ts_rank_cd = 0` rows must be kept or they drop out of the pool; needs a same-top-results check on sample queries |
| 2 | One `to_tsvector` per row (`CROSS JOIN LATERAL ... OFFSET 0`) in sessions, questions, learnings, lessons | about 200 ms (sessions 216 → 94-122, questions 84-100 → 38-52, learnings 52 → 23) | Small: four branches, local edits | Low: same results by construction |
| 3 | Stored tsvectors (trigger or side table) | 100-150 ms **on top of** fix 2 | Migration, trigger, backfill of the five sources; `sessions` is written often | Medium-high: write cost, backfill, and its gain mostly overlaps fix 2 |
| 4 | Filter mail to the project first, one MATERIALIZED CTE shared with the thread branch | 80-120 ms | Small to medium (a `project_id` column on `messages` is a migration; the CTE is not) | Low with the CTE |
| 5 | Keep models loaded (Ollama `keep_alive`, reranker `--exit-idle-time`) | 2.5-3.5 s on the first search after an idle gap; nothing when warm | Smallest: two settings, no code | Low; cost is VRAM, see below |

**VRAM cost of fix 5.** Embedder resident: 685 MB measured. Reranker (bge-reranker-v2-m3): the issue estimates its share, which I did not measure because it is stopped now. Total about 1.2-1.5 GB resident of 8,192 MiB; today 2,005 MiB are used with only the embedder loaded, so games keep about 6 GB. It is a trade for the operator, since the card is shared with games; a one-hour idle bound rather than "always" limits it to working hours.

## Recommendation

Order: **5 now, then 2, then 1, then 4; do not build 3.**

1. **Fix 5 first.** It needs no code, removes the largest single delay (2.5-3.5 s), and the reranker cold starts (29 a week) are what makes the first search of a work session slow. Set `OLLAMA_KEEP_ALIVE=1h` and `--exit-idle-time=3600`.
2. **Fix 2 next**: about 200 ms for a few local edits, with no result change, and it shrinks the later work on fix 1 (fewer expensive expressions to carry).
3. **Fix 1** is the one that moves the SQL under the 500 ms target (1.4-1.7 s → about 0.5-0.6 s on the issue's numbers); do it as one PR with the file split and a before/after of the top results for the sample queries.
4. **Fix 4** after 1: with narrow rows the mail scan is one of the few remaining costs, and by then 80-120 ms is a larger share. Use the CTE, not a column.
5. **Skip fix 3.** After fixes 1 and 2 the tsvector is computed only for surviving candidates, so 100-150 ms shrinks, while a trigger on a hot table and a backfill add a standing write cost.

Fixes 1, 2 and 4 together: 1.4-1.7 s → about 0.4-0.6 s (SQL), consistent with the issue's done-when 500 ms.

## Measure

Read from `/api/metrics` on omit after the deploy, on the temet-proxy project:

- `db_query_duration_ms_sum / _count{method="knowledge_search"}` ≤ **500 ms** (now 1.32 s overall; 1.4-1.7 s for the heavy any-term query, three runs).
- Whole call: `search_phase_duration_ms{phase="knowledge_search_handler"}` ≤ **1.5 s** warm. Today that phase has only 3 samples in 1,450 searches, so first check it is recorded on every call; if not, that gap is the first small fix and the measure meanwhile is `curl -w %{time_total}` on the issue's query (5.25 s first, 1.77 s repeated today).
- First search after 30 idle minutes ≤ warm + 0.5 s: `ollama ps` still lists the embedder, `journalctl --user -u mike-stack-rerank` shows fewer than 29 starts a week, and the first-call `curl` time is within 0.5 s of the repeat.
- Same top results for the sample queries before and after fixes 1 and 2.
