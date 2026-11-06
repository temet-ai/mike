SELECT id AS "id!", role AS "role!", bundle_id AS "bundle_id!", window_since AS "window_since!",
       window_until AS "window_until!", daily_call_cap AS "daily_call_cap!", status AS "status!",
       estimated_calls AS "estimated_calls!", estimated_usd, created_by AS "created_by!",
       created_at AS "created_at!", started_at, finished_at
  FROM eval.rejudge_runs
 ORDER BY id DESC
