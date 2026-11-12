WITH recent AS (
    SELECT id, route, kind, message, received_at,
           left(regexp_replace(split_part(btrim(message), E'\n', 1), '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS shape
      FROM wright.client_errors
     WHERE received_at >= now() - interval '24 hours'
)
SELECT 'page'::text AS scope,
       route || ' ' || kind || ': ' || shape AS resource,
       jsonb_build_object(
           'window', '24 hours',
           'route', route,
           'kind', kind,
           'reports', count(*),
           'first_at', min(received_at),
           'last_at', max(received_at),
           'last_report_id', max(id),
           'sample_message', (array_agg(message ORDER BY id DESC))[1]
       ) AS evidence
  FROM recent
 GROUP BY route, kind, shape
HAVING count(*) >= 3
 ORDER BY count(*) DESC, resource
 LIMIT 200
