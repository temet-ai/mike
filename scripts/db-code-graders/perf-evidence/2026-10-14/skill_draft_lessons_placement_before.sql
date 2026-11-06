SELECT l.id, k.code AS kind, l.text, t.user_prompt
  FROM memory.lessons l
  JOIN memory.lesson_kinds k ON k.id = l.kind_id
  JOIN memory.lesson_stages ls
    ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
  LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
 WHERE l.project_id = 993478
   AND l.topic = 'ui-design'
   AND l.agent_id IS NOT DISTINCT FROM NULL::bigint
   AND l.archived_at IS NULL
 ORDER BY l.id
