WITH errors AS (
    SELECT r.request_id, q.session_id, t.body,
           substring(t.body FROM '^([A-Za-z]+(:[^ ]*)?) hook error') AS hook,
           left(regexp_replace(split_part(substring(t.body FROM 'hook error:?\s*(.*)$'), E'\n', 1),
                               '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS reason
      FROM requests.requests q
      JOIN tools.results r ON r.request_id = q.id AND r.is_error
     CROSS JOIN LATERAL (
         SELECT btrim(CASE jsonb_typeof(r.content)
                          WHEN 'string' THEN r.content #>> '{}'
                          WHEN 'array' THEN (SELECT string_agg(b ->> 'text', E'\n')
                                               FROM jsonb_array_elements(r.content) b
                                              WHERE b ->> 'type' = 'text')
                          ELSE r.content::text
                      END) AS body
     ) t
     WHERE q.created_at >= now() - interval '24 hours'
       AND t.body ~ '^[A-Za-z]+(:[^ ]*)? hook error'
),
grouped AS (
    SELECT hook, reason,
           count(*) AS failures,
           count(DISTINCT session_id) AS sessions,
           max(request_id) AS last_request_id,
           (array_agg(body ORDER BY request_id DESC))[1] AS sample
      FROM errors
     GROUP BY hook, reason
)
SELECT 'hook'::text AS scope,
       hook || ': ' || coalesce(nullif(reason, ''), '(no reason given)') AS resource,
       jsonb_build_object(
           'window', '24 hours',
           'hook', hook,
           'reason', reason,
           'failures', failures,
           'sessions', sessions,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM grouped
 WHERE failures >= 10
 ORDER BY failures DESC, resource
 LIMIT 200
