UPDATE launch.tree_checks
   SET finished_at = now(),
       state_after = 'abandoned:unverified',
       output = output || $3
 WHERE id = $1
   AND finished_at IS NULL
   AND (run_id = $2 OR starts_with(run_id, $2 || '#'))
RETURNING id
