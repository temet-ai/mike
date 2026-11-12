-- Wright's checks become rows (temet-ai/mike#72): wright.checks holds each check's name, source,
-- cadence, expectation, probe kind, query and thresholds, so a check is listed, added, changed,
-- disabled and removed while mike runs.
--
-- probe 'sql': query is a read that returns (scope text, resource text, evidence jsonb) rows,
-- run in a READ ONLY transaction with params bound as $1 (jsonb). probe 'logs': params carry
-- level, repeats and window_minutes. probe 'cold_forks': a Rust probe over launch and request
-- rows; params carry window_hours and cache_ttl_minutes.
--
-- check_runs.check_name and problems.check_name reference the table: removing a check removes
-- its runs, problems and episodes; disabling it keeps them.

SET LOCAL lock_timeout = '10s';

CREATE TABLE wright.checks (
    name       text        PRIMARY KEY CHECK (name ~ '^[a-z][a-z0-9_]{2,62}$'),
    source     text        NOT NULL CHECK (source IN ('logs', 'capture_db', 'mcp_tools', 'page', 'hooks', 'workers', 'sessions')),
    cadence    text        NOT NULL CHECK (cadence IN ('event', 'regular', 'daily')),
    expects    text        NOT NULL CHECK (btrim(expects) <> ''),
    probe      text        NOT NULL CHECK (probe IN ('sql', 'logs', 'cold_forks')),
    query      text,
    params     jsonb       NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(params) = 'object'),
    enabled    boolean     NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT checks_query_only_for_sql CHECK ((probe = 'sql') = (query IS NOT NULL AND btrim(query) <> ''))
);

COMMENT ON TABLE wright.checks IS
    'Wright''s checks: one row per check, run by the watch worker when enabled and due (temet-ai/mike#72).';

INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('log_errors', 'logs', 'regular',
 'No ERROR log line of one target and shape repeats 3 times within an hour, and nothing panics',
 'logs', NULL,
 '{"level": "ERROR", "repeats": 3, "window_minutes": 60}'),
('log_warnings', 'logs', 'regular',
 'No WARN log line of one target and shape repeats 100 times within an hour',
 'logs', NULL,
 '{"level": "WARN", "repeats": 100, "window_minutes": 60}'),
('request_failures', 'capture_db', 'event',
 'No request error of one shape (wire error, aborted stream, upstream failure) recurs 5 times within an hour',
 'sql',
 $q$WITH recent AS (
    SELECT r.id, r.session_id, e.message,
           left(regexp_replace(split_part(btrim(e.message), E'\n', 1), '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS kind
      FROM requests.requests r
      JOIN requests.errors e ON e.request_id = r.id
     WHERE r.created_at >= now() - make_interval(mins => ($1->>'window_minutes')::int)
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
           'window', ($1->>'window_minutes') || ' minutes',
           'failed_requests', failed,
           'sessions', sessions,
           'first_request_id', first_request_id,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM kinds
 WHERE failed >= ($1->>'min_failures')::int
 ORDER BY failed DESC, kind
 LIMIT 200$q$,
 '{"window_minutes": 60, "min_failures": 5}'),
('undelivered_mail', 'capture_db', 'regular',
 'No session that kept working has 3 or more messages that were never sent, pushed or dead-lettered after 30 minutes',
 'sql',
 $q$WITH waiting AS (
    SELECT m.to_session_id, m.id, m.message_uuid, m.queued_at
      FROM mailbox.messages m
      JOIN sessions.sessions s ON s.id = m.to_session_id
     WHERE m.sent_request_id IS NULL
       AND m.sent_at IS NULL
       AND m.channel_delivered_at IS NULL
       AND m.consumed_at IS NULL
       AND m.dead_lettered_at IS NULL
       AND m.queued_at < now() - make_interval(mins => ($1->>'min_age_minutes')::int)
       AND m.queued_at >= now() - make_interval(days => ($1->>'window_days')::int)
       AND s.last_seen > m.queued_at + make_interval(mins => ($1->>'recipient_active_minutes')::int)
)
SELECT 'mail'::text AS scope,
       s.session_uuid::text AS resource,
       jsonb_build_object(
           'undelivered', count(*),
           'oldest_queued_at', min(w.queued_at),
           'recipient_last_seen', s.last_seen,
           'message_uuids', (array_agg(w.message_uuid ORDER BY w.id))[1:20]
       ) AS evidence
  FROM waiting w
  JOIN sessions.sessions s ON s.id = w.to_session_id
 GROUP BY s.session_uuid, s.last_seen
HAVING count(*) >= ($1->>'min_messages')::int
 ORDER BY count(*) DESC, resource
 LIMIT 200$q$,
 '{"min_age_minutes": 30, "window_days": 7, "recipient_active_minutes": 10, "min_messages": 3}'),
('mcp_tool_errors', 'mcp_tools', 'regular',
 'No mike MCP tool fails the same way 20 times a day or in 3 or more sessions',
 'sql',
 $q$WITH errors AS (
    SELECT u.tool_name, r.request_id, q.session_id, t.body,
           left(regexp_replace(split_part(btrim(t.body), E'\n', 1), '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS kind
      FROM requests.requests q
      JOIN tools.results r ON r.request_id = q.id AND r.is_error
      JOIN tools.uses u ON u.id = r.tool_use_id
     CROSS JOIN LATERAL (
         SELECT CASE jsonb_typeof(r.content)
                    WHEN 'string' THEN r.content #>> '{}'
                    WHEN 'array' THEN (SELECT string_agg(b ->> 'text', E'\n')
                                         FROM jsonb_array_elements(r.content) b
                                        WHERE b ->> 'type' = 'text')
                    ELSE r.content::text
                END AS body
     ) t
     WHERE q.created_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
       AND u.tool_name LIKE 'mcp\_\_mike\_\_%'
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
SELECT 'mcp_tool'::text AS scope,
       tool_name || ': ' || coalesce(nullif(kind, ''), '(empty error)') AS resource,
       jsonb_build_object(
           'window', ($1->>'window_hours') || ' hours',
           'tool', tool_name,
           'error_kind', kind,
           'failures', failures,
           'sessions', sessions,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM grouped
 WHERE failures >= ($1->>'min_failures')::int
    OR sessions >= ($1->>'min_sessions')::int
 ORDER BY failures DESC, resource
 LIMIT 200$q$,
 '{"window_hours": 24, "min_failures": 20, "min_sessions": 3}'),
('page_client_errors', 'page', 'regular',
 'No page route reports the same load, console, API or unhandled error 3 times a day',
 'sql',
 $q$WITH recent AS (
    SELECT id, route, kind, message, received_at,
           left(regexp_replace(split_part(btrim(message), E'\n', 1), '[0-9a-f]{8,}|[0-9]+', 'N', 'g'), 160) AS shape
      FROM wright.client_errors
     WHERE received_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
)
SELECT 'page'::text AS scope,
       route || ' ' || kind || ': ' || shape AS resource,
       jsonb_build_object(
           'window', ($1->>'window_hours') || ' hours',
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
HAVING count(*) >= ($1->>'min_reports')::int
 ORDER BY count(*) DESC, resource
 LIMIT 200$q$,
 '{"window_hours": 24, "min_reports": 3}'),
('hook_errors', 'hooks', 'regular',
 'No hook fails or refuses for the same reason 10 times a day',
 'sql',
 $q$WITH errors AS (
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
     WHERE q.created_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
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
           'window', ($1->>'window_hours') || ' hours',
           'hook', hook,
           'reason', reason,
           'failures', failures,
           'sessions', sessions,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM grouped
 WHERE failures >= ($1->>'min_failures')::int
 ORDER BY failures DESC, resource
 LIMIT 200$q$,
 '{"window_hours": 24, "min_failures": 10}'),
('llm_role_errors', 'workers', 'regular',
 'No LLM role fails half or more of at least 10 calls a day',
 'sql',
 $q$WITH calls AS (
    SELECT role,
           count(*) AS calls,
           count(*) FILTER (WHERE outcome IN ('http_error', 'timeout', 'transport_error')) AS failed,
           max(created_at) FILTER (WHERE outcome IN ('http_error', 'timeout', 'transport_error')) AS last_failed_at,
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
           'outcomes_seen', outcomes_seen
       ) AS evidence
  FROM calls
 WHERE calls >= ($1->>'min_calls')::int
   AND failed >= ($1->>'min_failure_share')::numeric * calls
 ORDER BY failed DESC, role
 LIMIT 200$q$,
 '{"window_hours": 24, "min_calls": 10, "min_failure_share": 0.5}'),
('session_tool_errors', 'sessions', 'regular',
 'No tool error of one shape hits 3 or more sessions 10 times a day',
 'sql',
 $q$WITH errors AS (
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
     WHERE q.created_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
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
           'window', ($1->>'window_hours') || ' hours',
           'tool', tool_name,
           'error_kind', kind,
           'failures', failures,
           'sessions', sessions,
           'last_request_id', last_request_id,
           'sample_error', sample
       ) AS evidence
  FROM grouped
 WHERE failures >= ($1->>'min_failures')::int
   AND sessions >= ($1->>'min_sessions')::int
 ORDER BY failures DESC, resource
 LIMIT 200$q$,
 '{"window_hours": 24, "min_failures": 10, "min_sessions": 3}'),
('scheduled_job_failures', 'workers', 'daily',
 'No scheduled job''s latest run on a machine is its second failure in a row',
 'sql',
 $q$WITH latest AS (
    SELECT DISTINCT ON (job_name, machine_uuid)
           id, job_name, machine_uuid, started_at, result_kind, exit_code, consecutive_failures,
           output_tail
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(days => ($1->>'window_days')::int)
     ORDER BY job_name, machine_uuid, started_at DESC, id DESC
)
SELECT 'worker'::text AS scope,
       'job:' || job_name || coalesce('@' || machine_uuid::text, '') AS resource,
       jsonb_build_object(
           'job', job_name,
           'machine_uuid', machine_uuid,
           'run_id', id,
           'started_at', started_at,
           'result_kind', result_kind,
           'exit_code', exit_code,
           'consecutive_failures', consecutive_failures,
           'output_tail', output_tail
       ) AS evidence
  FROM latest
 WHERE consecutive_failures >= ($1->>'min_consecutive_failures')::int
 ORDER BY consecutive_failures DESC, resource
 LIMIT 200$q$,
 '{"window_days": 7, "min_consecutive_failures": 2}'),
('cold_forks', 'capture_db', 'daily',
 'No fork of a warm template starts without reading the template''s prompt cache, and each cold fork names its cause',
 'cold_forks', NULL,
 '{"window_hours": 24, "cache_ttl_minutes": 60}');

DELETE FROM wright.problems WHERE check_name NOT IN (SELECT name FROM wright.checks);
DELETE FROM wright.check_runs WHERE check_name NOT IN (SELECT name FROM wright.checks);

ALTER TABLE wright.check_runs
    ADD CONSTRAINT check_runs_check_name_fkey
        FOREIGN KEY (check_name) REFERENCES wright.checks(name) ON DELETE CASCADE;
ALTER TABLE wright.problems
    ADD CONSTRAINT problems_check_name_fkey
        FOREIGN KEY (check_name) REFERENCES wright.checks(name) ON DELETE CASCADE;
