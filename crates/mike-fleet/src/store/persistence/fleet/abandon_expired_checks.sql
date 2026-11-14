UPDATE launch.tree_checks
   SET finished_at = now(),
       state_after = 'abandoned:unverified',
       output = output || $3
 WHERE finished_at IS NULL
   AND (run_id = $1 OR starts_with(run_id, $1 || '#'))
   AND started_at < now() - make_interval(secs => $2)
RETURNING id
