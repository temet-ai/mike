-- Two shared pieces of launch SQL, so the statements that use them read one definition.
--
-- launch.run_seats: a fleet run's seats, one row per run and session, from the session's
-- first seat launch in that run (`status` launches are not seats), with the seat's kind.
-- The run record's seat, mail, report, token, cargo-repeat and intervention statements each
-- carried this as their own `seats` CTE. Readers filter on run_id, which the planner pushes
-- below the DISTINCT ON into idx_launch_launches_run_created.
CREATE OR REPLACE VIEW launch.run_seats AS
SELECT DISTINCT ON (l.run_id, s.id)
       l.run_id, s.id AS session_id, s.session_uuid, l.seat,
       CASE WHEN l.seat LIKE 'coordinator%' THEN 'coordinator'
            WHEN l.seat LIKE 'observer%' THEN 'observer'
            WHEN l.seat LIKE 'acceptance%' THEN 'acceptance'
            WHEN l.seat LIKE 'template-%' THEN 'template'
            WHEN l.seat = 'reference' THEN 'reference'
            ELSE 'worker' END AS kind,
       l.created_at AS launched_at
  FROM launch.launches l
  JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
 WHERE l.run_id IS NOT NULL AND l.seat IS NOT NULL AND l.seat <> 'status'
 ORDER BY l.run_id, s.id, l.created_at, l.id;

COMMENT ON VIEW launch.run_seats IS
    'A fleet run''s seats: one row per (run_id, session), from the session''s first seat launch in the run, status launches excluded. Filter on run_id.';

-- memory.seat_sessions_touched(launch_after, event_after, turn_after): the launched sessions
-- with a project whose seat memory may have changed since the previous refresh's marks. A
-- session counts when it has a launch above launch_after, a fleet event above event_after
-- (directly or through its run), or a turn closed or a turn fact built after turn_after; every
-- session of a counted session's runs counts with it, since brief dedupe, Stop-hook feedback
-- collapse and verdicts are per run. A NULL launch_after returns every launched session with a
-- project (the daily full sweep). This is the change feed refresh.sql used to carry inline.
CREATE OR REPLACE FUNCTION memory.seat_sessions_touched(
    p_launch_after bigint,
    p_event_after bigint,
    p_turn_after timestamptz
)
RETURNS TABLE (session_uuid uuid)
LANGUAGE sql
STABLE
PARALLEL SAFE
AS $function$
WITH touched AS (
    SELECT l.session_uuid FROM launch.launches l WHERE l.id > p_launch_after
    UNION
    SELECT e.session_uuid FROM launch.fleet_events e
     WHERE e.id > p_event_after AND e.session_uuid IS NOT NULL
    UNION
    SELECT l.session_uuid FROM launch.launches l
     WHERE l.run_id IN (SELECT e.run_id FROM launch.fleet_events e WHERE e.id > p_event_after)
    UNION
    SELECT s.session_uuid
      FROM sessions.sessions s
     WHERE s.id IN (SELECT ot.session_id FROM memory.operator_turns ot
                     WHERE ot.closed_at > p_turn_after
                    UNION ALL
                    SELECT tf.session_id FROM memory.turn_facts tf
                     WHERE tf.built_at > p_turn_after)
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
    SELECT l.session_uuid FROM launch.launches l WHERE p_launch_after IS NULL
)
SELECT c.session_uuid
  FROM candidates c
 WHERE EXISTS (SELECT 1 FROM sessions.sessions s
                WHERE s.session_uuid = c.session_uuid AND s.project_id IS NOT NULL)
$function$;

COMMENT ON FUNCTION memory.seat_sessions_touched(bigint, bigint, timestamptz) IS
    'Launched sessions with a project whose seat memory may have changed after the given launch id, fleet event id and turn time, with every session of their runs; NULL launch_after returns all. Feeds the seat-memory refresh.';
