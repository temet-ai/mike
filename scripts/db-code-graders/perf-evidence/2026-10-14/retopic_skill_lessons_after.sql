UPDATE memory.lessons l
   SET topic = 'ui-design-system'
  FROM unnest(ARRAY['ui-design']::text[], ARRAY['ui-design-system']::text[]) AS m(from_topic, to_topic),
       memory.lesson_placements lp
 WHERE lp.id = l.placement_id AND lp.code = 'skill'
   AND l.project_id = 993478
   AND l.topic = m.from_topic
   AND l.archived_at IS NULL
