WITH calls AS (
    SELECT role,
           count(*) AS calls,
           count(*) FILTER (WHERE outcome IN ('http_error', 'timeout', 'transport_error')) AS failed,
           max(created_at) FILTER (WHERE outcome IN ('http_error', 'timeout', 'transport_error')) AS last_failed_at,
           to_jsonb(array_agg(DISTINCT outcome ORDER BY outcome)) AS outcomes_seen
      FROM llm.calls
     WHERE created_at >= now() - interval '24 hours'
     GROUP BY role
)
SELECT 'worker'::text AS scope,
       'llm_role:' || role AS resource,
       jsonb_build_object(
           'window', '24 hours',
           'role', role,
           'calls', calls,
           'failed', failed,
           'failure_share', round(failed::numeric / calls, 3),
           'last_failed_at', last_failed_at,
           'outcomes_seen', outcomes_seen
       ) AS evidence
  FROM calls
 WHERE calls >= 10
   AND failed * 2 >= calls
 ORDER BY failed DESC, role
 LIMIT 200
