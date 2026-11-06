INSERT INTO launch.tree_checks (run_id, state, command, askers)
VALUES ($1, $2, $3, ARRAY[$4::text])
RETURNING id
