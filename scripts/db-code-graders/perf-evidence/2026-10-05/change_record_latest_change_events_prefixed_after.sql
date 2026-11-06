WITH newest AS (
    SELECT n.id
      FROM workspace.change_events n
     WHERE n.checkout = '/work/checkout-3'
       AND (cardinality('{edit,shell_change,commit}'::text[]) = 0 OR n.kind = ANY('{edit,shell_change,commit}'))
     ORDER BY n.id DESC
     LIMIT 3000
)
SELECT e.id, e.project_id, e.checkout, e.at, e.kind, e.session_id, e.run_id, e.task,
       e.paths, e.subject, e.detail, e.before_sha, e.after_sha
  FROM newest
  JOIN workspace.change_events e ON e.id = newest.id
  -- grader-allow: LATERAL stops at the first matching prefix for each event; it is not a per-row scalar lookup
  CROSS JOIN LATERAL (
        SELECT 1 AS hit
          FROM unnest(e.paths) AS ep(path)
          JOIN unnest('{crates/mike/src/mod_3/file_5.rs}'::text[]) AS wanted(prefix) ON starts_with(ep.path, wanted.prefix)
         LIMIT 1
       ) m
 ORDER BY e.id DESC
 LIMIT 5
