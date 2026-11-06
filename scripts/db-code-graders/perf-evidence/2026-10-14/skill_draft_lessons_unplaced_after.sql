SELECT l.id, k.code AS kind, l.text, t.user_prompt
  FROM memory.lessons l
  JOIN memory.lesson_kinds k ON k.id = l.kind_id
  JOIN memory.lesson_stages ls
    ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
  LEFT JOIN memory.lesson_placements lp ON lp.id = l.placement_id
  LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
 WHERE l.project_id = 993478
   AND l.topic = 'ui-design'
   AND l.archived_at IS NULL
   AND (l.placement_id IS NULL OR lp.code = 'skill')
 ORDER BY l.id
