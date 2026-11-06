SELECT c.path, c.seen_at
  FROM launch.tree_changes c
 WHERE c.run_id = $1
   AND c.seen_at > $2
 ORDER BY c.seen_at, c.id
 LIMIT 5000
