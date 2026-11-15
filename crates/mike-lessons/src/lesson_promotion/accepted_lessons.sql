SELECT l.id, l.text, l.topic, l.source_turn_id, st.code AS "stage!", src.code AS "source!",
       k.code AS "kind!", l.project_id AS "project_id!", p.project_uuid, p.project_path, l.agent_id,
       a.name AS "agent_name?", pl.code AS "placement?",
       l.placement_covered_by AS covered_by, l.revision
  FROM memory.lessons l
  JOIN projects.projects p ON p.id = l.project_id
  JOIN memory.lesson_stages st ON st.id = l.stage_id
  JOIN memory.lesson_sources src ON src.id = l.source_id
  JOIN memory.lesson_kinds k ON k.id = l.kind_id
  LEFT JOIN memory.lesson_placements pl ON pl.id = l.placement_id
  LEFT JOIN agents.agents a ON a.id = l.agent_id
 WHERE l.archived_at IS NULL AND l.retired_at IS NULL
   AND (l.expires_at IS NULL OR l.expires_at > now())
   AND (l.machine_uuid IS NULL OR l.machine_uuid = $1)
   AND (st.code IN ('daily', 'long_term') OR (st.code = 'raw' AND src.code = 'operator'))
   AND p.project_path <> '(no project)'
   AND NOT EXISTS (
       SELECT 1 FROM memory.lesson_relations rel
         JOIN memory.lesson_relation_kinds rk ON rk.id = rel.kind_id AND rk.code = 'supersedes'
         JOIN memory.lesson_relation_statuses rs ON rs.id = rel.status_id AND rs.code = 'confirmed'
        WHERE rel.to_lesson_id = l.id)
 ORDER BY p.project_uuid, l.id
