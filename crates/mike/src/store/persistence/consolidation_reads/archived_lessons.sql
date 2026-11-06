SELECT l.id AS "id!", l.uuid::text AS "uuid!", l.project_id, p.project_path,
       lk.code AS "kind!", l.text AS "text!", l.topic,
       l.session_id, s.session_uuid::text AS "session_uuid?", l.source_turn_id,
       l.agent_id, l.created_at AS "created_at!", l.archived_at, ar.code AS "archived_reason?",
       l.duplicate_of_id
  FROM memory.lessons l
  LEFT JOIN projects.projects p ON p.id = l.project_id
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
  LEFT JOIN sessions.sessions s ON s.id = l.session_id
  LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
 WHERE p.project_path = $1
   AND l.archived_at IS NOT NULL
 ORDER BY l.archived_at DESC
