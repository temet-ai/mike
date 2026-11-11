WITH recent AS (
    SELECT r.id, r.session_id, e.message,
           left(regexp_replace(split_part(btrim(e.message), E'\n', 1), '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS kind
      FROM requests.requests r
      JOIN requests.errors e ON e.request_id = r.id
     WHERE r.created_at >= now() - interval '1 hour'
),
kinds AS (
    SELECT kind,
           count(*) AS failed,
           count(DISTINCT session_id) AS sessions,
           min(id) AS first_request_id,
           max(id) AS last_request_id,
           (array_agg(message ORDER BY id DESC))[1] AS sample
      FROM recent
     GROUP BY kind
)
SELECT 'capture'::text AS scope,
       kind AS resource,
       jsonb_build_object(
           'window', '1 hour',
           'failed_requests', failed,
           'sessions', sessions,
           'first_request_id', first_request_id,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM kinds
 WHERE failed >= 5
 ORDER BY failed DESC, kind
 LIMIT 200
