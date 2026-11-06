SELECT id, run_id, state, state_after, command, started_at, finished_at, exit_code, output, askers, told
  FROM launch.tree_checks
 WHERE run_id = 'fleet-perf-1'
 ORDER BY id DESC LIMIT 1
