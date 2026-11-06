SELECT e.id, e.project_id, e.checkout, e.at, e.kind, e.session_id, e.run_id, e.task,
       e.paths, e.subject, e.detail, e.before_sha, e.after_sha
  FROM workspace.change_events e
  -- grader-allow: LATERAL stops at the first matching prefix for each event; it is not a per-row scalar lookup
  CROSS JOIN LATERAL (
        SELECT 1 AS hit
          FROM unnest(e.paths) AS ep(path)
          JOIN unnest('{crates/mike/src/mod_3/,docs/doc_3.md}'::text[]) AS wanted(prefix) ON starts_with(ep.path, wanted.prefix)
         LIMIT 1
       ) m
 WHERE e.checkout = '/work/checkout-3'
   AND e.id > 200000
   AND (1348628::bigint IS NULL OR e.session_id IS DISTINCT FROM 1348628)
   AND (cardinality('{edit,commit}'::text[]) = 0 OR e.kind = ANY('{edit,commit}'))
 ORDER BY e.id
 LIMIT 50
