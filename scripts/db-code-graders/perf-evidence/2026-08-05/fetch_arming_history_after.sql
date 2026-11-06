WITH closed AS (
    SELECT t.id, tot.code
      FROM memory.operator_turns t
      JOIN memory.turn_outcomes  tot ON tot.id = t.outcome_id
     WHERE t.session_id = 397909 AND t.closed_at IS NOT NULL
), recent AS (
    SELECT code FROM closed ORDER BY id DESC LIMIT 20
)
SELECT
    (SELECT COUNT(*) FROM closed)::bigint AS closed_turn_count,
    COALESCE(
        (SELECT array_agg(code) FROM recent),
        ARRAY[]::text[]
    ) AS recent_outcomes_newest_first;
