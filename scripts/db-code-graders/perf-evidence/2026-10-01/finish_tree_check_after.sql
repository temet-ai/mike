UPDATE launch.tree_checks SET finished_at = now(), exit_code = 0, output = 'ok', state_after = 'perf-state' WHERE id = 5
