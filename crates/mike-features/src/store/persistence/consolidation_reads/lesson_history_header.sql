SELECT l.id AS "lesson_id!", l.uuid::text AS "lesson_uuid!", l.text AS "lesson_text!",
       lk.code AS "kind!", ls.code AS "stage!", l.created_at AS "created_at!",
       s.session_uuid::text AS "session_uuid?", l.source_turn_id
  FROM memory.lessons l
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
  JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  LEFT JOIN sessions.sessions s ON s.id = l.session_id
 WHERE l.id = $1
