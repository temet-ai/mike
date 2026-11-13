WITH recent AS (
        SELECT t.id AS turn_id, t.session_id
          FROM memory.operator_turns t
         WHERE t.created_at >= $1
           AND t.closed_at IS NOT NULL
           AND NOT EXISTS (
               SELECT 1
                 FROM memory.turn_facts_backstop_attempts a
                WHERE a.operator_turn_id = t.id
                  AND (a.unbuildable_at IS NOT NULL OR a.retry_after > now()))
    ), newest_message AS (
        SELECT c.session_id, max(c.classified_at) AS newest
          FROM prompt_kinds.classifications c
         WHERE c.session_id IN (SELECT session_id FROM recent)
         GROUP BY c.session_id
    ), stale AS (
        SELECT r.session_id, r.turn_id
          FROM recent r
          LEFT JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
         WHERE f.operator_turn_id IS NULL
        UNION
        SELECT r.session_id, r.turn_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
         WHERE f.builder_version <> $2
        UNION
        SELECT r.session_id, r.turn_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
          JOIN requests.requests q ON q.operator_turn_id = r.turn_id
                                  AND q.id > f.max_request_id
        UNION
        SELECT r.session_id, r.turn_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
          JOIN sessions.sessions s ON s.id = r.session_id
         WHERE f.next_state = 'pending'
           AND s.last_seen < $3
        UNION
        SELECT r.session_id, r.turn_id
          FROM recent r
          JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
          JOIN newest_message n ON n.session_id = r.session_id
         WHERE f.next_state = 'pending'
           AND n.newest > f.built_at
    ), oldest_build AS (
        SELECT r.session_id,
               min(COALESCE(f.built_at, '-infinity'::timestamptz)) AS oldest
          FROM recent r
          LEFT JOIN memory.turn_facts f ON f.operator_turn_id = r.turn_id
         WHERE r.session_id IN (SELECT session_id FROM stale)
         GROUP BY r.session_id
    ), chosen AS (
        SELECT o.session_id, o.oldest
          FROM oldest_build o
         ORDER BY o.oldest ASC, o.session_id DESC
         LIMIT $4
    ), attempted AS (
        INSERT INTO memory.turn_facts_backstop_attempts AS a
               (operator_turn_id, attempt_count, last_attempt_at, retry_after)
        SELECT st.turn_id, 1, now(), now() + interval '60 seconds'
          FROM stale st
          JOIN chosen c ON c.session_id = st.session_id
        ON CONFLICT (operator_turn_id) DO UPDATE
           SET attempt_count = a.attempt_count + 1,
               last_attempt_at = now(),
               retry_after = now() + least(interval '1 hour',
                                           interval '60 seconds' * power(2, a.attempt_count)),
               unbuildable_at = CASE WHEN a.attempt_count + 1 >= $5 THEN now() END
    )
    SELECT c.session_id AS "session_id!"
      FROM chosen c
     ORDER BY c.oldest ASC, c.session_id DESC
