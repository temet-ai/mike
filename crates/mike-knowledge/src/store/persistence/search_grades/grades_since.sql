SELECT g.event_id AS event_id, g.candidates AS candidates, g.used_references AS used_references,
       g.relevant_references AS relevant_references
  FROM memory.search_grades g
 WHERE g.graded_at >= $1
 ORDER BY g.event_id
