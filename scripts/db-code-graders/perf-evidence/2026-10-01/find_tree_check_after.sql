SELECT id, run_id, state, state_after, command, started_at, finished_at, exit_code, output, askers, told
  FROM launch.tree_checks
 WHERE run_id = 'fleet-perf-1' AND state = '18d8042386b79e2c279fd162df0205c8' AND finished_at IS NOT NULL
   AND (state_after IS NULL OR state_after = state)
 ORDER BY id DESC LIMIT 1
