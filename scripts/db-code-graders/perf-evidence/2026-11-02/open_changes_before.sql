WITH newest AS (
    SELECT n.id
      FROM workspace.change_events n
     WHERE n.checkout = '/mnt/data/projects/temet/temet-proxy'
     ORDER BY n.id DESC
     LIMIT 3000
), recent AS MATERIALIZED (
    SELECT r.id, r.session_id, r.at, r.kind, r.paths
      FROM workspace.change_events r
     WHERE r.id = ANY(ARRAY(SELECT id FROM newest))
       AND r.kind IN ('edit', 'shell_change', 'commit')
), flat AS (
    SELECT p.path, e.id, e.kind, e.session_id, e.at
      FROM recent e
      CROSS JOIN LATERAL unnest(e.paths) AS p(path)
), marked AS (
    SELECT f.path, f.id, f.kind, f.session_id, f.at,
           max(f.id) FILTER (WHERE f.kind = 'commit') OVER (PARTITION BY f.path) AS last_commit,
           row_number() OVER (PARTITION BY f.path, (f.kind = 'commit') ORDER BY f.id DESC) AS rank
      FROM flat f
)
SELECT m.path AS "path!", m.session_id, m.at AS "at!"
  FROM marked m
 WHERE m.kind <> 'commit'
   AND m.rank = 1
   AND (m.last_commit IS NULL OR m.id > m.last_commit)
 ORDER BY m.at DESC, m.path
