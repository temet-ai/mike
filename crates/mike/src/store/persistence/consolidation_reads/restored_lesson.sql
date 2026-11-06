SELECT l.id AS "id!", l.project_id, p.project_path, lk.code AS "kind!", l.text AS "text!",
       l.agent_id, ls.code AS "stage!", l.source_turn_id, l.created_at AS "created_at!",
       l.expires_at, l.topic
  FROM memory.lessons l
  LEFT JOIN projects.projects p ON p.id = l.project_id
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
  JOIN memory.lesson_stages ls ON ls.id = l.stage_id
 WHERE l.id = $1
