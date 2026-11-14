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
 WHERE id = $1
