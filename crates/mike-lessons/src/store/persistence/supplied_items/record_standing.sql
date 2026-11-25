WITH turns AS MATERIALIZED (
    SELECT t.id, t.session_id, t.start_request_id, t.created_at, s.project_id
      FROM memory.operator_turns t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id AND f.prompt_kind = 'genuine'
      JOIN sessions.sessions s ON s.id = t.session_id
     WHERE t.closed_at >= $1 AND t.closed_at < $2
),
standing AS MATERIALIZED (
    SELECT l.id, l.project_id, l.text, l.embedding, l.embedding_model, l.created_at
      FROM memory.lessons l
      JOIN memory.lesson_routes r ON r.lesson_id = l.id AND r.route = 'standing'
     WHERE l.archived_at IS NULL AND l.retired_at IS NULL AND l.embedding IS NOT NULL
),
near AS (
    SELECT t.id AS turn_id, t.session_id, t.start_request_id, t.created_at, l.id AS lesson_id, l.text,
           min(i.embedding <=> l.embedding) AS distance
      FROM turns t
      JOIN memory.semantic_items i
        ON i.operator_turn_id = t.id AND i.kind IN ('turn_prompt', 'turn_ending')
      JOIN standing l
        ON l.project_id = t.project_id
       AND l.created_at < t.created_at
       AND l.embedding_model = split_part(i.embedder_key, '|', 1)
     WHERE i.embedding IS NOT NULL
     GROUP BY t.id, t.session_id, t.start_request_id, t.created_at, l.id, l.text
),
ranked AS (
    SELECT n.*, row_number() OVER (PARTITION BY n.turn_id ORDER BY n.distance, n.lesson_id) AS rn,
           row_number() OVER (PARTITION BY n.session_id, n.lesson_id ORDER BY n.distance, n.turn_id) AS once
      FROM near n
     WHERE n.distance <= $3
)
INSERT INTO memory.supplied_items
       (session_id, request_id, item_kind, lesson_id, route, score, chance, item_text, supplied_at)
SELECT r.session_id, r.start_request_id, 'lesson', r.lesson_id, 'standing', 1 - r.distance, NULL,
       r.text, r.created_at
  FROM ranked r
 WHERE r.rn <= $4 AND r.once = 1
   AND NOT EXISTS (
       SELECT 1 FROM memory.supplied_items seen
        WHERE seen.session_id = r.session_id
          AND seen.lesson_id = r.lesson_id
          AND seen.route = 'standing')
