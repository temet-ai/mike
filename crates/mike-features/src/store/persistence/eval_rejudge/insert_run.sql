INSERT INTO eval.rejudge_runs
    (bundle_id, window_since, window_until, daily_call_cap, estimated_calls, estimated_usd, created_by, role)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING id AS "id!"
