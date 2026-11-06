INSERT INTO launch.tree_checks (run_id, state, command, askers)
VALUES ('fleet-perf-1', 'perf-state-new', 'make check', ARRAY['db-dev-T1'::text])
RETURNING id
