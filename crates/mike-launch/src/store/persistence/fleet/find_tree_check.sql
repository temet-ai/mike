SELECT id,
       run_id,
       state,
       state_after,
       command,
       started_at,
       finished_at,
       exit_code,
       output,
       askers,
       told
  FROM launch.tree_checks
 WHERE run_id = $1
   AND state = $2
   AND finished_at IS NOT NULL
   AND (state_after IS NULL OR state_after = state)
 ORDER BY id DESC
 LIMIT 1
