WITH errors AS (
    SELECT u.tool_name, r.request_id, q.session_id, t.body,
           left(regexp_replace(split_part(btrim(regexp_replace(t.body, '^Exit code [0-9]+\s*', '')), E'\n', 1),
                               '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS kind
      FROM requests.requests q
      JOIN tools.results r ON r.request_id = q.id AND r.is_error
      JOIN tools.uses u ON u.id = r.tool_use_id
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
       AND u.tool_name NOT LIKE 'mcp\_\_mike\_\_%'
       AND t.body !~ '^[A-Za-z]+(:[^ ]*)? hook error'
),
grouped AS (
    SELECT tool_name, kind,
           count(*) AS failures,
           count(DISTINCT session_id) AS sessions,
           max(request_id) AS last_request_id,
           (array_agg(body ORDER BY request_id DESC))[1] AS sample
      FROM errors
     GROUP BY tool_name, kind
)
SELECT 'session_tool'::text AS scope,
       tool_name || ': ' || coalesce(nullif(kind, ''), '(empty error)') AS resource,
       jsonb_build_object(
           'window', '24 hours',
           'tool', tool_name,
           'error_kind', kind,
           'failures', failures,
           'sessions', sessions,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM grouped
 WHERE failures >= 10
   AND sessions >= 3
 ORDER BY failures DESC, resource
 LIMIT 200
