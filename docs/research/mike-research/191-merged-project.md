---
title: "#191 merged project resolves to the empty row"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: Still reproduces; 6 of 140 searches in 14 days named a merged project; recommend one shared resolver (option b) starting with resolve.sql.
session_id:
  - 35313732-fea7-4b90-81aa-e3cb880df3e8
---

## What is true now (2026-11-15, running build)

- Reproduces. `GET /api/knowledge/search?project=/Users/dawiddutoit/projects/temet/temet-proxy&q=brief` returns 409 "ambiguous … mike (a37cee7a…); mike (ec5bcb17…)". `project=a37cee7a-c329-4c81-bee5-03a5b942313a` returns 200 with `items: []` and `project_uuid` a37cee7a (the empty merged row). (The 8-character prefix alone is "unknown project", so the issue's second curl needs the full UUID.)
- No fix has landed: `git log --since=2026-11-13` on `crates/mike-knowledge/src/store/persistence/project_knowledge/` shows only the crate extraction. The file moved from `crates/mike/...` (the issue's path) to `crates/mike-knowledge/src/store/persistence/project_knowledge/resolve.sql`.
- resolve.sql lines 2-8: levels 1 (UUID), 2 (numeric id), 3 (path, and `locations.root_path`) match merged rows; only levels 4 and 5 (lines 12, 17) filter `merged_into_id IS NULL`. `resolve.rs:68-86` errors when `candidates.len() > 1`, so a merged row plus its live target is ambiguous.
- Data: 32 merged rows (5 of them `/Users/...` Mac paths); 18 of the 32 share a folder name with a live project. Merged rows own 0 locations, 0 sessions and 0 lessons, so nothing lives on them: they are pure aliases left behind. That also means option (c) has nothing to repoint.
- Impact: `tools.uses` for `search_project_knowledge` in the last 14 days: 140 calls; 6 named a merged project (4.3%): 3 by path (409, retried), 3 by merged UUID (empty result): ids 1296575, 1336396, 1336398, 1336683, 1375633, 1382378, latest 2026-11-14 09:35. All six name the same merged project (313870 -> 993478). Search events (`memory.search_events`) do not store the project, so this is counted from tool inputs.
- Other lookups that ignore `merged_into_id` (they return the merged row's own id):
  - `crates/mike-base/src/store/persistence/workspace/project_lookup.rs:98-150` (`resolve_project`, query at :132-147) and `:92` (`find_project_id_by_path`). Callers: `mike-launch/src/api/workspace/roster.rs:107`, `mike-launch/src/api/handlers/mailbox_thread_create.rs:246`, `mike-fleet/src/api/fleet/graph.rs:97`, `mike-features/src/proxy_tools/roster.rs:31`, `mike-fleet/src/launch/fleet/wake.rs:91`, `brief_notes_sync.rs:207`. A Mac path resolves to the empty row for roster, thread creation, and fleet wake.
  - `crates/mike-lessons/src/store/persistence/lesson_relations.rs:347-361` (`lookup_project_id_by_path`, no filter; CLI `config/subcommands/lesson_relations.rs:192`).
  - `session_scope.rs:51` and `lesson_create.rs:16` go through `find_knowledge_project`, so they share resolve.sql's fault (`resolve_lesson_project` could attach a lesson to a merged row).
- Already correct: `mike-base/.../project_placement.rs:27,38,108,123`, `session_projects.rs:41`, `meta.rs:20`, `mike-launch/.../work_threads/project.rs:16` and `thread_summaries_tail.sql:98` all use `COALESCE(merged_into_id, id)`. `locations.rs:33`, `rehydrate/projects.rs:45` and `unknown_project_error` (`resolve.rs:98`) hide merged rows, which is right for listings.

## Options

| | (a) COALESCE in resolve.sql only | (b) one shared resolver | (c) repoint locations off merged rows |
|---|---|---|---|
| Work | one file: map each hit to `COALESCE(p.merged_into_id, p.id)`, `SELECT DISTINCT` (about 6 lines) + one test in `resolve_tests.rs` | one SQL fn or `ProjectResolver` in mike-base used by resolve.sql, `project_lookup.rs`, `lesson_relations.rs`: about 3 files plus 6 call sites (no signature change if it stays in the existing methods) | migration; merged rows have 0 locations, so it changes nothing |
| Fixes the 409 and empty result (the 6 of 140 searches) | yes | yes | no: the merged row's own `project_path` and `project_uuid` still match |
| Leaves broken | roster, thread create, fleet wake and lesson CLI still hit the empty row for a merged path | nothing, if all resolvers adopt it | everything |
| Risk | low; `find_knowledge_project` is also used by lessons, which then follow merges (desired) | medium: roster uses suffix matching and an id cache (`id_cache.project`), which must cache the resolved id | none |

## Recommendation

Do (a) now, as the issue proposes (the 409 and the empty result are the live harm), and in the same PR make (b): put the merge-following in one place (`COALESCE(merged_into_id, id)` with distinct ids) that `project_lookup.rs::fetch_path_rows` and `lookup_project_id_by_path` also use, since the same Mac path misbehaves in roster, mailbox thread creation and fleet wake. Skip (c). Keep the listing filters as they are. Dedup must happen before the `candidates.len() > 1` check, or merged-plus-target still counts as two.

## Measure

1. The issue's two curls (path, and full merged UUID) return 200 with items from project 993478 (`ec5bcb17`), no 409.
2. `tools.uses` for `search_project_knowledge` whose `project` input matches a row with `merged_into_id` set: the 14-day count falls from 6 to 0 resolving to an empty row. Join `tools.uses.input->>'project'` to `projects.projects.project_uuid` or `project_path`.
3. A test pins: merged path, merged UUID and numeric merged id each resolve to the live project; `find_project_id_by_path('/Users/dawiddutoit/projects/temet/temet-proxy')` returns 993478.
