SELECT a.id AS "lesson_a_id!", a.text AS "lesson_a_text!", a.created_at AS "lesson_a_created_at!",
       b.id AS "lesson_b_id!", b.text AS "lesson_b_text!", b.created_at AS "lesson_b_created_at!",
       (a.embedding <=> b.embedding)::float8 AS "distance!"
  FROM memory.lessons a
  JOIN memory.lessons b ON b.project_id = a.project_id AND b.id > a.id
  JOIN memory.lesson_stages sa ON sa.id = a.stage_id AND sa.code IN ('daily', 'long_term')
  JOIN memory.lesson_stages sb ON sb.id = b.stage_id AND sb.code IN ('daily', 'long_term')
 WHERE a.project_id = $1
   AND a.embedding IS NOT NULL AND b.embedding IS NOT NULL
   AND ((a.embedding <=> b.embedding) < $2
        OR b.text ~ ('\m' || a.id || '\M')
        OR a.text ~ ('\m' || b.id || '\M'))
   AND NOT EXISTS (
       SELECT 1 FROM memory.lesson_relations r
        WHERE (r.from_lesson_id = a.id AND r.to_lesson_id = b.id)
           OR (r.from_lesson_id = b.id AND r.to_lesson_id = a.id)
   )
 ORDER BY (a.embedding <=> b.embedding)
