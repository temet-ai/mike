-- Wright repair 22 (finding 23) tried to rewrite memory/seat refresh.sql for
-- slow_queries problem 61944 and was refused as too large (11 files, 717
-- lines). Its target was replaced in e9d9c8c47 (the refresh recomputes only
-- the seat sessions that changed) and composed through the views
-- launch.run_seats and memory.seat_sessions_touched in 8b00cb608, so the
-- repair can never land and the problem it serves is gone.
--
-- The repair becomes `dropped` with that reason; problem 61944 and the
-- wright_repairs problem that names repair 22 are closed as healthy with their
-- open episodes. Each update matches only the live rows in their current
-- state, so on any other database this migration changes nothing.

UPDATE wright.repairs
   SET state = 'dropped',
       reason = 'superseded: e9d9c8c47 replaced the target statement (refresh.sql) and 8b00cb608 composed its replacement through views; the refused patch can no longer land',
       updated_at = now()
 WHERE id = 22
   AND state = 'refused'
   AND problem_id = 61944;

WITH healed AS (
    UPDATE wright.problems
       SET state = 'healthy', healthy_at = now()
     WHERE state = 'open'
       AND (   (id = 61944 AND check_name = 'slow_queries')
            OR (id = 63694 AND check_name = 'wright_repairs' AND resource = '22'))
    RETURNING id
)
UPDATE wright.episodes e
   SET closed_at = now()
  FROM healed
 WHERE e.problem_id = healed.id
   AND e.closed_at IS NULL;
