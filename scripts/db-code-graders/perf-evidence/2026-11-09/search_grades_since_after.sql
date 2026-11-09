SELECT g.event_id, g.candidates, g.used_references
  FROM memory.search_grades g
 WHERE g.graded_at >= now() - interval '14 days'
 ORDER BY g.event_id;
