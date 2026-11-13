WITH newest AS (
    SELECT n.id
      FROM workspace.change_events n
     WHERE n.checkout = $1
     ORDER BY n.id DESC
     LIMIT 3000
), recent AS MATERIALIZED (
    SELECT r.id, r.session_id, r.at, r.kind, r.paths
      FROM workspace.change_events r
     WHERE r.id = ANY(ARRAY(SELECT id FROM newest))
       AND r.kind IN ('edit', 'shell_change', 'commit')
), last_per_path AS (
    SELECT p.path,
           max(e.id) FILTER (WHERE e.kind <> 'commit') AS last_change,
           max(e.id) FILTER (WHERE e.kind = 'commit') AS last_commit
      FROM recent e
      CROSS JOIN LATERAL unnest(e.paths) AS p(path)
     GROUP BY p.path
)
SELECT l.path AS "path!", e.session_id, e.at AS "at!"
  FROM last_per_path l
  JOIN recent e ON e.id = l.last_change
 WHERE l.last_commit IS NULL OR l.last_change > l.last_commit
 ORDER BY e.at DESC, l.path
