-- tool_contract_errors: a rejected call keeps its problem only for quiet_hours after the newest
-- failure of that tool and error kind. Before, one failure was reported again on every pass
-- for the whole window_hours (168), so a call already fixed held a problem open for a week
-- (#315). The counts still cover window_hours. The evidence now names the request that made
-- the newest failing call and its tool_use id, not the request that carried the result.
-- omit got this check live through POST /api/wright/checks, so the row is written here for
-- every database; stored params are kept and only gain quiet_hours.
INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('tool_contract_errors', 'mcp_tools', 'regular',
 'No MCP tool (mike''s, tether''s or any other) rejects a call''s arguments: a missing, unknown or mistyped field is a schema or description to fix the first time it happens; a rejection is reported for quiet_hours after the newest one',
 'sql',
 $q$WITH errors AS (
    SELECT u.tool_name, u.request_id, u.wire_tool_use_id, q.created_at AS failed_at,
           q.session_id, t.body,
           left(regexp_replace(split_part(btrim(t.body), E'\n', 1), '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS kind
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
     WHERE q.created_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
       AND u.tool_name LIKE 'mcp\_\_%'
       AND t.body ~* '(invalid arguments|missing field|unknown field|invalid type|InputValidationError|needs `[a-z_]+`|requires `[a-z_]+`)'
),
grouped AS (
    SELECT tool_name, kind,
           count(*) AS failures,
           count(DISTINCT session_id) AS sessions,
           max(failed_at) AS last_failure_at,
           (array_agg(request_id ORDER BY failed_at DESC, request_id DESC))[1] AS last_request_id,
           (array_agg(wire_tool_use_id ORDER BY failed_at DESC, request_id DESC))[1] AS last_tool_use_id,
           (array_agg(body ORDER BY failed_at DESC, request_id DESC))[1] AS sample
      FROM errors
     GROUP BY tool_name, kind
)
SELECT 'tool_contract'::text AS scope,
       tool_name || ': ' || kind AS resource,
       jsonb_build_object(
           'window', ($1->>'window_hours') || ' hours',
           'tool', tool_name,
           'error_kind', kind,
           'failures', failures,
           'sessions', sessions,
           'last_failure_at', last_failure_at,
           'last_request_id', last_request_id,
           'last_tool_use_id', last_tool_use_id,
           'sample_error', sample
       ) AS evidence
  FROM grouped
 WHERE failures >= ($1->>'min_failures')::int
   AND last_failure_at >= now() - make_interval(hours => COALESCE(($1->>'quiet_hours')::int, 24))
 ORDER BY failures DESC, resource
 LIMIT 200$q$,
 '{"min_failures": 1, "window_hours": 168, "quiet_hours": 24}')
ON CONFLICT (name) DO UPDATE
   SET expects = EXCLUDED.expects,
       query = EXCLUDED.query,
       params = EXCLUDED.params || wright.checks.params,
       updated_at = now();
