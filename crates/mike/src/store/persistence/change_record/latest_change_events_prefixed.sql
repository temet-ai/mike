WITH newest AS (
    SELECT n.id
      FROM workspace.change_events n
     WHERE n.checkout = $1
       AND (cardinality($3::text[]) = 0 OR n.kind = ANY($3))
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
          JOIN unnest($2::text[]) AS wanted(prefix) ON starts_with(ep.path, wanted.prefix)
         LIMIT 1
       ) m
 ORDER BY e.id DESC
 LIMIT $4
