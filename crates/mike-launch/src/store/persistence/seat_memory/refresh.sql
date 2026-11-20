-- Refreshes memory.seat_memory_items from memory.seat_memory(uuid[]).
-- $1, $2: the highest launch and fleet event ids the previous refresh saw; $3: its time mark.
-- A seat session with a newer launch or fleet event, or a turn closed or a turn fact built
-- after $3, is recomputed together with every session of its runs, and only those sessions'
-- items are pruned. Mail reaches a seat as a channel turn, so its turn scopes it. The time mark
-- handed back is a minute before this statement's start, so a turn whose transaction commits
-- after this snapshot is still seen next time. All three NULL recomputes every seat session
-- (the worker's daily sweep). The marks to pass next time come back in the last three columns.
WITH touched AS (
    SELECT l.session_uuid FROM launch.launches l WHERE l.id > $1::bigint
    UNION
    SELECT e.session_uuid FROM launch.fleet_events e
     WHERE e.id > $2::bigint AND e.session_uuid IS NOT NULL
    UNION
    SELECT l.session_uuid FROM launch.launches l
     WHERE l.run_id IN (SELECT e.run_id FROM launch.fleet_events e WHERE e.id > $2::bigint)
    UNION
    SELECT s.session_uuid
      FROM sessions.sessions s
     WHERE s.id IN (SELECT ot.session_id FROM memory.operator_turns ot
                     WHERE ot.closed_at > $3::timestamptz
                    UNION ALL
                    SELECT tf.session_id FROM memory.turn_facts tf
                     WHERE tf.built_at > $3::timestamptz)
), candidates AS (
    SELECT l.session_uuid
      FROM launch.launches l
     WHERE l.session_uuid IN (SELECT t.session_uuid FROM touched t)
    UNION
    SELECT r2.session_uuid
      FROM launch.launches r
      JOIN launch.launches r2 ON r2.run_id = r.run_id
     WHERE r.session_uuid IN (SELECT t.session_uuid FROM touched t)
    UNION
    SELECT l.session_uuid FROM launch.launches l WHERE $1::bigint IS NULL
), seat_sessions AS MATERIALIZED (
    SELECT c.session_uuid
      FROM candidates c
     WHERE EXISTS (SELECT 1 FROM sessions.sessions s
                    WHERE s.session_uuid = c.session_uuid AND s.project_id IS NOT NULL)
), fresh AS MATERIALIZED (
    SELECT DISTINCT ON (sm.project_id, sm.item_kind, sm.source_ref, sm.session_uuid) sm.*
      FROM (SELECT array_agg(k.session_uuid) AS sessions FROM seat_sessions k) keys
     CROSS JOIN LATERAL memory.seat_memory(keys.sessions) sm
     WHERE sm.session_uuid IS NOT NULL AND sm.source_ref IS NOT NULL AND sm.text IS NOT NULL
     ORDER BY sm.project_id, sm.item_kind, sm.source_ref, sm.session_uuid, sm.at
), changed AS (
    SELECT f.*
      FROM fresh f
      LEFT JOIN memory.seat_memory_items i
        ON i.project_id = f.project_id AND i.item_kind = f.item_kind
       AND i.source_ref = f.source_ref AND i.session_uuid = f.session_uuid
     WHERE i.project_id IS NULL
        OR (i.session_id, i.run_id, i.seat, i.task, i.author, i.from_session, i.from_role, i.at,
            i.text, i.repeat_count, i.report_filed, i.verdict_accepted)
           IS DISTINCT FROM
           (f.session_id, f.run_id, f.seat, f.task, f.author, f.from_session, f.from_role, f.at,
            f.text, f.repeat_count, f.report_filed, f.verdict_accepted)
), written AS (
    INSERT INTO memory.seat_memory_items AS i
        (project_id, item_kind, source_ref, session_uuid, session_id, run_id, seat, task, author,
         from_session, from_role, at, text, repeat_count, report_filed, verdict_accepted)
    SELECT f.project_id, f.item_kind, f.source_ref, f.session_uuid, f.session_id, f.run_id, f.seat,
           f.task, f.author, f.from_session, f.from_role, f.at, f.text, f.repeat_count,
           f.report_filed, f.verdict_accepted
      FROM changed f
    ON CONFLICT (project_id, item_kind, source_ref, session_uuid) DO UPDATE
       SET session_id = EXCLUDED.session_id, run_id = EXCLUDED.run_id, seat = EXCLUDED.seat,
           task = EXCLUDED.task, author = EXCLUDED.author, from_session = EXCLUDED.from_session,
           from_role = EXCLUDED.from_role, at = EXCLUDED.at, text = EXCLUDED.text,
           repeat_count = EXCLUDED.repeat_count, report_filed = EXCLUDED.report_filed,
           verdict_accepted = EXCLUDED.verdict_accepted, refreshed_at = now()
    RETURNING 1
), removed AS (
    DELETE FROM memory.seat_memory_items i
     WHERE ($1::bigint IS NULL
            OR i.session_uuid IN (SELECT k.session_uuid FROM seat_sessions k))
       AND NOT EXISTS (SELECT 1 FROM fresh f
                        WHERE f.project_id = i.project_id AND f.item_kind = i.item_kind
                          AND f.source_ref = i.source_ref AND f.session_uuid = i.session_uuid)
    RETURNING 1
)
SELECT (SELECT count(*) FROM fresh) AS "items!",
       (SELECT count(*) FROM written) AS "written!",
       (SELECT count(*) FROM removed) AS "removed!",
       (SELECT COALESCE(max(l.id), 0) FROM launch.launches l) AS "launch_mark!",
       (SELECT COALESCE(max(e.id), 0) FROM launch.fleet_events e) AS "event_mark!",
       now() - interval '1 minute' AS "turn_mark!"
