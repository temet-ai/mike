SELECT e.id, e.project_id, e.checkout, e.at, e.kind, e.session_id, e.run_id, e.task,
       e.paths, e.subject, e.detail, e.before_sha, e.after_sha
  FROM workspace.change_events e
 WHERE e.checkout = '/work/checkout-3'
   AND e.id > 245000
   AND (NULL::bigint IS NULL OR e.session_id IS DISTINCT FROM NULL)
   AND (cardinality('{}'::text[]) = 0 OR e.kind = ANY('{}'))
 ORDER BY e.id
 LIMIT 50
