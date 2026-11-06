-- 20260929180000 added projects.projects.merged_into_id without statistics.
-- Until the table is analysed the planner estimates one project row for
-- `merged_into_id IS NULL`, nests the per-project request aggregate inside a
-- loop, and list_projects goes from ~45 ms to ~3.7 s (measured on the live
-- capture DB, 2026-09-29). Autovacuum does not analyse after an ALTER, so do it
-- here.

ANALYZE projects.projects;
