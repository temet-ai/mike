SELECT id, run_id, command, started_at
  FROM launch.tree_checks
 WHERE finished_at IS NULL
   AND (run_id = $1 OR starts_with(run_id, $1 || '#'))
 ORDER BY started_at, id
