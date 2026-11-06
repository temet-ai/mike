-- A request belongs only to an operator turn of its own session (#619).
-- Inheritance through spawn_session and cross-session agent instances had
-- linked spawned sessions' requests, and their spawned sessions' requests in
-- turn, to the spawning session's turn. The row-change notify trigger is held
-- off for this one historical repair: it would send one notification per
-- relinked row (17,935 on the capture database on 2026-11-01) and doubles the
-- migration's time.
SET LOCAL jit = off;

ALTER TABLE requests.requests DISABLE TRIGGER trg_requests_row_changed;

CREATE TEMP TABLE unlinked_turns ON COMMIT DROP AS
WITH unlinked AS (
    UPDATE requests.requests r
       SET operator_turn_id = NULL
      FROM memory.operator_turns t
     WHERE t.id = r.operator_turn_id
       AND t.session_id <> r.session_id
    RETURNING t.id AS turn_id
)
SELECT DISTINCT turn_id FROM unlinked;

ALTER TABLE requests.requests ENABLE TRIGGER trg_requests_row_changed;

SELECT memory.refresh_inherited_turns(array_agg(turn_id))
  FROM unlinked_turns
HAVING count(*) > 0;
