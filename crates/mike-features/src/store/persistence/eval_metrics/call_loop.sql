SELECT s.at AS "at!", q.ok_calls AS "ok_calls!", q.refused AS "refused!",
       q.verdict_rows AS "verdict_rows!", q.sessions_at_cap AS "sessions_at_cap!",
       q.top_sessions AS "top_sessions!"
  FROM unnest($1::timestamptz[]) AS s(at)
 CROSS JOIN LATERAL (
    WITH calls AS (
        SELECT count(*) FILTER (WHERE outcome = 'ok') AS ok_calls,
               count(*) FILTER (WHERE outcome = 'refused') AS refused
          FROM llm.calls
         WHERE role = 'session_judge' AND created_at > s.at - interval '24 hours' AND created_at <= s.at
    ),
    verdict_rows AS (
        SELECT count(*) AS n
          FROM session_judges.verdicts
         WHERE mode = 'live' AND judged_at > s.at - interval '24 hours' AND judged_at <= s.at
    ),
    capped AS (
        SELECT count(*) AS n
          FROM session_judges.attempts
         WHERE window_started_at > s.at - interval '24 hours' AND window_attempts >= $2
    ),
    top_sessions AS (
        SELECT session_id, count(*) AS calls
          FROM llm.calls
         WHERE role = 'session_judge' AND session_id IS NOT NULL
           AND created_at > s.at - interval '24 hours' AND created_at <= s.at
         GROUP BY session_id
         ORDER BY calls DESC, session_id
         LIMIT 10
    )
    SELECT c.ok_calls, c.refused, vr.n AS verdict_rows, cp.n AS sessions_at_cap,
           COALESCE(jsonb_agg(jsonb_build_object('session_id', ts.session_id, 'calls', ts.calls)
                              ORDER BY ts.calls DESC, ts.session_id)
                    FILTER (WHERE ts.session_id IS NOT NULL), '[]'::jsonb) AS top_sessions
      FROM calls c
      CROSS JOIN verdict_rows vr
      CROSS JOIN capped cp
      LEFT JOIN top_sessions ts ON true
     GROUP BY c.ok_calls, c.refused, vr.n, cp.n
 ) q
 ORDER BY s.at
