-- Three Wright checks that ask whether work achieves anything, not only whether it fails or is slow.
--
-- idle_polls: a statement run by a role in params.roles at least calls_per_hour times an hour that
-- averages max_rows_per_call rows or fewer a call (pg_stat_statements rows: returned or changed).
-- Windowed like slow_queries: a statement with no prior episode is judged on its history since
-- stats_since; a flagged one keeps base_calls, base_rows, base_total_ms and base_at in its evidence,
-- rolls the base once it is older than window_hours, and carries the last verdict until the new
-- window is min_window_minutes old. Utility statements, pings and statements matching skip_text
-- (another application on this database) are skipped.
--
-- steering_work_without_reader: steerer LLM calls made after their steerer was disabled or deleted
-- (current enabled, deleted_at and updated_at), and acceptance-criteria writer calls made after the
-- last verifier was turned off while none is enabled.
--
-- stale_launches: open launches the launch reaper (launch/gone_closure.rs) should have closed:
-- released released_minutes ago after the launch began, or older than hours with no open
-- registration and no session activity in the last hours, never-opened launches included.
-- Production already holds all three (added through the API), hence ON CONFLICT DO NOTHING.
--
-- stale_launches replaces orphan_launches, which saw only launches whose registration had been
-- reaped, and its remedy close_orphan_launches (action close_orphan_launch), which closed those
-- quietly and so hid 75 launches left open since September. The launch reaper now closes every
-- gone launch (launch/gone_closure.rs). The check, the remedy, the action code, the tidy kind
-- that only it used, and every row that
-- refers to them go: investigations of its problems and their findings, its follow-ups, its inbox
-- entries, then the remedy (its actions and changes cascade) and the check (its runs, problems,
-- episodes and causes cascade). Investigations and follow-ups only lose the reference on a
-- cascade (ON DELETE SET NULL), so they are deleted first.
SET LOCAL lock_timeout = '10s';

DELETE FROM wright.investigations i
 WHERE i.problem_id IN (SELECT p.id FROM wright.problems p WHERE p.check_name = 'orphan_launches')
    OR i.followup_id IN (SELECT f.id FROM wright.followups f WHERE f.check_name = 'orphan_launches');

DELETE FROM wright.followups WHERE check_name = 'orphan_launches';

DELETE FROM wright.inbox WHERE check_name = 'orphan_launches';

DELETE FROM wright.remedies
 WHERE name = 'close_orphan_launches' OR action = 'close_orphan_launch';

DELETE FROM wright.checks WHERE name = 'orphan_launches';

ALTER TABLE wright.remedies
    DROP CONSTRAINT remedies_kind_matches_action,
    DROP CONSTRAINT remedies_action_check,
    DROP CONSTRAINT remedies_kind_check,
    ADD CONSTRAINT remedies_kind_check CHECK (kind IN ('setting', 'notice')),
    ADD CONSTRAINT remedies_action_check CHECK (
        action IN ('notify_undelivered_sender', 'set_setting', 'disable_filter_rule')),
    ADD CONSTRAINT remedies_kind_matches_action CHECK (
        (action = 'notify_undelivered_sender' AND kind = 'notice')
        OR (action IN ('set_setting', 'disable_filter_rule') AND kind = 'setting'));

INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params, enabled) VALUES
('idle_polls', 'capture_db', 'regular',
 'No mike statement runs calls_per_hour or more times an hour while it averages max_rows_per_call rows or fewer a call over its window, so frequent statements find work to do',
 'sql',
 $q$WITH stats AS (
    SELECT s.queryid,
           sum(s.calls) AS calls,
           sum(s.rows) AS rows,
           sum(s.total_exec_time) AS total_ms,
           min(s.stats_since) AS stats_since,
           (array_agg(s.query ORDER BY s.calls DESC))[1] AS query
      FROM pg_stat_statements s
     WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
       AND s.toplevel
       AND s.queryid IS NOT NULL
       AND s.userid IN (SELECT r.oid FROM pg_roles r
                         WHERE r.rolname IN (SELECT jsonb_array_elements_text($1->'roles')))
       AND s.query !~* '^\s*(begin|commit|rollback|end|start\s+transaction|savepoint|release|set|reset|show|listen|unlisten|notify|discard|deallocate|prepare|explain|vacuum|analyze)\M'
       AND s.query !~* '^\s*select\s+(1|[$]1)\s*;?\s*$'
       AND s.query !~* ($1->>'skip_text')
     GROUP BY s.queryid
),
prior AS (
    SELECT DISTINCT ON (e.evidence->>'queryid') e.evidence->>'queryid' AS queryid, e.evidence AS pe
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id
     WHERE p.check_name = 'idle_polls'
     ORDER BY e.evidence->>'queryid', e.id DESC
),
based AS (
    SELECT st.*, pr.pe,
           (pr.pe IS NULL
            OR (pr.pe->>'calls')::numeric > st.calls
            OR (pr.pe->>'counted_since')::timestamptz IS DISTINCT FROM st.stats_since) AS fresh
      FROM stats st
      LEFT JOIN prior pr ON pr.queryid = st.queryid::text
),
base AS (
    SELECT b.*,
           CASE WHEN b.fresh THEN 0 ELSE (b.pe->>'base_calls')::numeric END AS base_calls,
           CASE WHEN b.fresh THEN 0 ELSE (b.pe->>'base_rows')::numeric END AS base_rows,
           CASE WHEN b.fresh THEN 0 ELSE (b.pe->>'base_total_ms')::numeric END AS base_total_ms,
           CASE WHEN b.fresh THEN b.stats_since ELSE (b.pe->>'base_at')::timestamptz END AS base_at
      FROM based b
),
windowed AS (
    SELECT w.*,
           w.calls - w.base_calls AS window_calls,
           w.rows - w.base_rows AS window_rows,
           (w.total_ms - w.base_total_ms) / 1000 AS window_seconds,
           extract(epoch FROM now() - w.base_at) / 3600 AS window_length_hours,
           now() - w.base_at >= make_interval(mins => ($1->>'min_window_minutes')::int) AS ready
      FROM base w
),
judged AS (
    SELECT j.*,
           CASE WHEN j.ready THEN j.window_calls / j.window_length_hours
                ELSE (j.pe->>'calls_per_hour')::numeric END AS calls_per_hour,
           CASE WHEN j.ready THEN j.window_rows / nullif(j.window_calls, 0)
                ELSE (j.pe->>'window_rows_per_call')::numeric END AS rows_per_call,
           j.base_at < now() - make_interval(hours => ($1->>'window_hours')::int) AS rolls
      FROM windowed j
)
SELECT 'query'::text AS scope,
       'query ' || j.queryid || ': '
           || left(regexp_replace(btrim(j.query), '\s+', ' ', 'g'), 120) AS resource,
       jsonb_build_object(
           'queryid', j.queryid::text,
           'calls_per_hour', round(j.calls_per_hour, 1),
           'window_rows_per_call', round(j.rows_per_call, 5),
           'judged_on', CASE WHEN j.ready THEN 'this window' ELSE 'the previous window' END,
           'window_since', j.base_at,
           'window_hours', round(j.window_length_hours, 2),
           'window_calls', j.window_calls,
           'window_rows', j.window_rows,
           'window_seconds', round(j.window_seconds::numeric, 1),
           'calls', j.calls,
           'rows', j.rows,
           'total_seconds', round((j.total_ms / 1000)::numeric, 1),
           'counted_since', j.stats_since,
           'base_calls', CASE WHEN j.rolls THEN j.calls ELSE j.base_calls END,
           'base_rows', CASE WHEN j.rolls THEN j.rows ELSE j.base_rows END,
           'base_total_ms', round((CASE WHEN j.rolls THEN j.total_ms ELSE j.base_total_ms END)::numeric, 1),
           'base_at', CASE WHEN j.rolls THEN now() ELSE j.base_at END,
           'query', left(regexp_replace(btrim(j.query), '\s+', ' ', 'g'), 160),
           'thresholds', $1
       ) AS evidence
  FROM judged j
 WHERE j.calls_per_hour >= ($1->>'calls_per_hour')::numeric
   AND j.rows_per_call <= ($1->>'max_rows_per_call')::numeric
 ORDER BY j.calls_per_hour DESC, j.queryid
 LIMIT 200$q$,
 '{"roles": ["devuser"], "skip_text": "tenders|awards", "calls_per_hour": 60, "max_rows_per_call": 0.02, "min_window_minutes": 30, "window_hours": 6}', true),
('steering_work_without_reader', 'capture_db', 'regular',
 'No steerer LLM call in the last hours ran for a steerer that was disabled or deleted at the time, and no acceptance-criteria writer call ran while no verifier steerer was enabled',
 'sql',
 $q$WITH verifier AS (
    SELECT bool_or(st.enabled AND st.deleted_at IS NULL) AS any_enabled,
           max(COALESCE(st.deleted_at, st.updated_at)) AS last_off_at
      FROM steering.steerers st
     WHERE st.kind = 'verifier'
),
calls AS (
    SELECT c.id, c.created_at, c.latency_ms, c.steerer_id,
           COALESCE(st.name, 'steerer ' || c.steerer_id || ' (row gone)') AS steerer_name, st.kind,
           COALESCE(st.deleted_at, st.updated_at) AS disabled_at
      FROM llm.calls c
      LEFT JOIN steering.steerers st ON st.id = c.steerer_id
     WHERE c.role = 'steerer'
       AND c.created_at >= now() - make_interval(hours => ($1->>'hours')::int)
       AND c.steerer_id IS NOT NULL
       AND NOT COALESCE(st.enabled AND st.deleted_at IS NULL, false)
       AND c.created_at >= COALESCE(st.deleted_at, st.updated_at, '-infinity'::timestamptz)
    UNION ALL
    SELECT c.id, c.created_at, c.latency_ms, NULL, 'criteria writer', 'verifier', v.last_off_at
      FROM llm.calls c
     CROSS JOIN verifier v
     WHERE c.role = 'steerer'
       AND c.created_at >= now() - make_interval(hours => ($1->>'hours')::int)
       AND c.steerer_id IS NULL
       AND NOT COALESCE(v.any_enabled, false)
       AND c.created_at >= COALESCE(v.last_off_at, '-infinity'::timestamptz)
       AND COALESCE(c.request_body->>'system', c.request_body->'messages'->0->>'content')
           LIKE 'You write acceptance criteria%'
)
SELECT 'steerer'::text AS scope,
       c.steerer_name AS resource,
       jsonb_build_object(
           'steerer_id', min(c.steerer_id),
           'kind', min(c.kind),
           'disabled_at', min(c.disabled_at),
           'window_hours', ($1->>'hours')::int,
           'calls', count(*),
           'latency_seconds', round(sum(COALESCE(c.latency_ms, 0)) / 1000.0, 1),
           'first_call', min(c.created_at),
           'last_call', max(c.created_at),
           'last_call_id', max(c.id)
       ) AS evidence
  FROM calls c
 GROUP BY c.steerer_name
 ORDER BY count(*) DESC, c.steerer_name
 LIMIT 200$q$,
 '{"hours": 24}', true),
('stale_launches', 'capture_db', 'regular',
 'Every launch is closed once its session has been released for released_minutes, or once it is older than hours with no open registration and no session activity in the last hours',
 'sql',
 $q$WITH due AS (
    SELECT l.id, l.backend, l.created_at, l.opened, l.run_id, l.seat, l.session_uuid,
           r.session_id IS NOT NULL AS registered,
           r.released_at, r.reaped_at,
           act.observed_at AS last_activity_at
      FROM launch.launches l
      LEFT JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
      LEFT JOIN workspace.session_registrations r ON r.session_id = s.id
      LEFT JOIN LATERAL runtime.session_activity(s.id) act ON TRUE
     WHERE l.closed_at IS NULL
       AND ((r.released_at >= l.created_at
             AND r.released_at < now() - make_interval(mins => ($1->>'released_minutes')::int))
            OR (l.created_at < now() - make_interval(hours => ($1->>'hours')::int)
                AND (r.session_id IS NULL OR r.released_at IS NOT NULL)
                AND NOT COALESCE(act.observed_at >= now() - make_interval(hours => ($1->>'hours')::int), FALSE)))
)
SELECT 'launch'::text AS scope,
       d.id::text AS resource,
       jsonb_build_object(
           'launch_id', d.id,
           'backend', d.backend,
           'created_at', d.created_at,
           'opened', d.opened,
           'run_id', d.run_id,
           'seat', d.seat,
           'session_uuid', d.session_uuid,
           'registered', d.registered,
           'released_at', d.released_at,
           'reaped_at', d.reaped_at,
           'last_activity_at', d.last_activity_at,
           'thresholds', $1
       ) AS evidence
  FROM due d
 ORDER BY d.id
 LIMIT 200$q$,
 '{"hours": 26, "released_minutes": 30}', true)
ON CONFLICT (name) DO NOTHING;
