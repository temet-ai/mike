-- GitHub issue #39: the `pin_project` / `set_project_ttl` autocreate
-- INSERTs wrote the raw `project_path` into `display_name` instead of
-- deriving the trimmed operator-facing name
-- (`extract::request::project_display_name` / `compute_project_root`).
-- Both call sites are fixed going forward (persistence/mod.rs); this
-- backfills existing rows so a project pinned or TTL'd before the fix
-- also shows a trimmed name in the /projects UI, not the full path.
--
-- Mirrors `project_display_name`'s Rust logic exactly: collapse repeated
-- '/' separators, trim leading/trailing '/', split on '/', and drop the
-- first two segments only when there are more than two (the assumed
-- `/Users/<username>` or `/home/<username>` prefix). Two segments or
-- fewer — including the sentinel "(no project)", which has exactly one —
-- is a no-op, matching the Rust function's own threshold.
--
-- Idempotent: `IS DISTINCT FROM` means a re-run after the derived value
-- already matches touches zero rows.
WITH parsed AS (
    SELECT id,
           regexp_split_to_array(
               trim(both '/' FROM regexp_replace(project_path, '/+', '/', 'g')),
               '/'
           ) AS segs
      FROM projects.projects
),
derived AS (
    SELECT id,
           CASE
               WHEN array_length(segs, 1) > 2
                   THEN array_to_string(segs[3:array_length(segs, 1)], '/')
               ELSE array_to_string(segs, '/')
           END AS derived_display_name
      FROM parsed
)
UPDATE projects.projects p
   SET display_name = derived.derived_display_name
  FROM derived
 WHERE p.id = derived.id
   AND p.display_name IS DISTINCT FROM derived.derived_display_name;
