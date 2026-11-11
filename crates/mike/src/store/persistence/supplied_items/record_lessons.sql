INSERT INTO memory.supplied_items
       (session_id, request_id, item_kind, lesson_id, route, score, chance, item_text)
SELECT s.id, $1, 'lesson', l.id, u.route, u.score, 1.0, l.text
  FROM UNNEST($2::bigint[], $3::text[], $4::float8[]) AS u(lesson_id, route, score)
  JOIN memory.lessons l ON l.id = u.lesson_id
  JOIN sessions.sessions s ON s.session_uuid = $5
 WHERE NOT EXISTS (
       SELECT 1 FROM memory.supplied_items seen
        WHERE seen.session_id = s.id
          AND seen.lesson_id = l.id
          AND seen.route = u.route
          AND (u.route <> 'proxy' OR seen.request_id = $1))
