SELECT e.id AS "id!",
       e.paths[1] AS "path!",
       e.before_sha,
       e.after_sha
  FROM workspace.change_events e
 WHERE e.checkout = $1
   AND e.id > $2
   AND e.kind IN ('edit', 'shell_change')
   AND cardinality(e.paths) = 1
 ORDER BY e.id
 LIMIT $3
