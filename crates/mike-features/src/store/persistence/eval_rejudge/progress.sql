WITH run AS (
    SELECT $1::bigint AS bundle_id, $2::timestamptz AS window_since, $3::timestamptz AS window_until
),
window_turns AS (
    SELECT t.id, t.judge_failure_reason, t.outcome_source,
           (f.prompt_kind = 'genuine' AND f.session_class <> 'synthetic'
            AND q.claude_code_agent_id IS NULL
            AND f.next_state IN ('resolved', 'none_session_settled')) AS eligible
      FROM run
      JOIN memory.operator_turns t
        ON t.closed_at >= run.window_since AND t.closed_at < run.window_until
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
      JOIN requests.requests q ON q.id = t.start_request_id
),
verdicts AS (
    SELECT v.operator_turn_id,
           count(*) AS n,
           sum(v.cost_usd) FILTER (WHERE v.created_at >= run.window_until) AS cost,
           count(*) FILTER (WHERE v.created_at >= run.window_until
                              AND v.created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC') AS today
      FROM run
      JOIN memory.turn_verdicts v
        ON v.bundle_id = run.bundle_id AND v.mode = 'live' AND v.reaction IS NOT NULL
      JOIN window_turns w ON w.id = v.operator_turn_id
     GROUP BY v.operator_turn_id
)
SELECT count(*) FILTER (WHERE w.eligible) AS "eligible_total!",
       count(*) FILTER (WHERE NOT w.eligible) AS "ineligible_total!",
       count(*) FILTER (WHERE w.eligible AND vd.n IS NOT NULL) AS "judged!",
       count(*) FILTER (WHERE w.eligible AND vd.n IS NULL
                          AND w.judge_failure_reason = 'permanent') AS "permanent_failures!",
       count(*) FILTER (WHERE w.eligible AND vd.n IS NULL
                          AND w.judge_failure_reason IS DISTINCT FROM 'permanent') AS "remaining!",
       COALESCE(sum(vd.today), 0)::bigint AS "calls_today!",
       COALESCE(sum(vd.cost), 0)::float8 AS "cost_usd_so_far!",
       count(*) FILTER (WHERE NOT w.eligible AND w.outcome_source = 'no_verdict') AS "ineligible_reprojected!"
  FROM window_turns w
  LEFT JOIN verdicts vd ON vd.operator_turn_id = w.id
