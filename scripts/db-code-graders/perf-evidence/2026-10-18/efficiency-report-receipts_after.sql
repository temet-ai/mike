SELECT id, run_id, started_at, finished_at, exit_code,
       coalesce(array_length(askers, 1), 0) AS consumers
FROM launch.tree_checks
WHERE (run_id = 'fleet-agent-dock-0a1f434b' OR starts_with(run_id, 'fleet-agent-dock-0a1f434b' || '#'))
  AND started_at >= '2026-10-13T12:50:17.411953Z'::timestamptz AND started_at <= '2026-10-13T17:01:17.150459Z'::timestamptz
ORDER BY id
