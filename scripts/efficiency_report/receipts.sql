SELECT id, run_id, started_at, finished_at, exit_code,
       coalesce(array_length(askers, 1), 0) AS consumers
FROM launch.tree_checks
WHERE (run_id = :'run' OR starts_with(run_id, :'run' || '#'))
  AND started_at >= :'since'::timestamptz AND started_at <= :'until'::timestamptz
ORDER BY id
