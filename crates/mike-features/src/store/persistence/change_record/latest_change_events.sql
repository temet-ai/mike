SELECT e.id, e.project_id, e.checkout, e.at, e.kind, e.session_id, e.run_id, e.task,
       e.paths, e.subject, e.detail, e.before_sha, e.after_sha
  FROM workspace.change_events e
 WHERE e.checkout = $1
   AND (cardinality($2::text[]) = 0 OR e.kind = ANY($2))
 ORDER BY e.id DESC
 LIMIT $3
