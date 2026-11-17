-- llm_role_errors flags a role only while it is still failing: its newest call in the window
-- failed. A role that failed for hours and then recovered heals on the next pass instead of
-- staying open until the failures age out of the 24-hour window.
UPDATE wright.checks
   SET expects = 'No LLM role is failing now: a role whose newest call failed has failed half or more of at least 10 calls in the window',
       query = $q$WITH calls AS (
    SELECT role,
           count(*) AS calls,
           count(*) FILTER (WHERE outcome IN ('http_error', 'timeout', 'transport_error')) AS failed,
           max(created_at) FILTER (WHERE outcome IN ('http_error', 'timeout', 'transport_error')) AS last_failed_at,
           max(created_at) FILTER (WHERE outcome = 'ok') AS last_ok_at,
           to_jsonb(array_agg(DISTINCT outcome ORDER BY outcome)) AS outcomes_seen
      FROM llm.calls
     WHERE created_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
     GROUP BY role
)
SELECT 'worker'::text AS scope,
       'llm_role:' || role AS resource,
       jsonb_build_object(
           'window', ($1->>'window_hours') || ' hours',
           'role', role,
           'calls', calls,
           'failed', failed,
           'failure_share', round(failed::numeric / calls, 3),
           'last_failed_at', last_failed_at,
           'last_ok_at', last_ok_at,
           'outcomes_seen', outcomes_seen
       ) AS evidence
  FROM calls
 WHERE calls >= ($1->>'min_calls')::int
   AND failed >= ($1->>'min_failure_share')::numeric * calls
   AND (last_ok_at IS NULL OR last_failed_at > last_ok_at)
 ORDER BY failed DESC, role
 LIMIT 200$q$,
       updated_at = now()
 WHERE name = 'llm_role_errors';
