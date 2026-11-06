WITH recent AS (
        SELECT t.id AS turn_id, t.session_id
          FROM memory.operator_turns t
         WHERE t.created_at >= '2026-09-22 15:42:00+00'
           AND t.closed_at IS NOT NULL
    ), newest_message AS (
        SELECT c.session_id, max(c.classified_at) AS newest
          FROM prompt_kinds.classifications c
         WHERE c.session_id IN (SELECT session_id FROM recent)
         GROUP BY c.session_id
    ), stale AS (
        SELECT r.session_id
          FROM recent r
          LEFT JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
         WHERE f.operator_turn_id IS NULL
        UNION
        SELECT r.session_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
         WHERE f.builder_version <> 'turn-facts-v1'
        UNION
        SELECT r.session_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
          JOIN requests.requests q ON q.operator_turn_id = r.turn_id
                                  AND q.id > f.max_request_id
        UNION
        SELECT r.session_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
          JOIN sessions.sessions s ON s.id = r.session_id
         WHERE f.next_state = 'pending'
           AND s.last_seen < '2026-09-24 15:12:00+00'
        UNION
        SELECT r.session_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
          JOIN newest_message n ON n.session_id = r.session_id
         WHERE f.next_state = 'pending'
           AND n.newest > f.built_at
    )
    SELECT session_id
      FROM stale
     ORDER BY session_id DESC
     LIMIT 25
