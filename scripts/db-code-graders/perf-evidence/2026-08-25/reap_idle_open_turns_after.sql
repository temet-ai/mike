WITH stale AS (
     SELECT t.id
       FROM memory.operator_turns t
       JOIN sessions.sessions s ON s.id = t.session_id
      WHERE t.closed_at IS NULL
        AND s.last_seen < now() - interval '30 minutes'
      FOR UPDATE OF t SKIP LOCKED
 )
 UPDATE memory.operator_turns t
    SET closed_at     = NOW(),
        outcome_id    = tot.id,
        classified_at = NOW()
   FROM memory.turn_outcomes tot, stale
  WHERE tot.code = 'unknown'
    AND t.id     = stale.id
  RETURNING t.id
