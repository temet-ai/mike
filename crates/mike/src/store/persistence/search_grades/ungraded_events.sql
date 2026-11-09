SELECT e.id AS "event_id!", e.query AS "query!", e.kind_filter,
       e.used_references AS "used_references!",
       p.id AS "project_id!", p.project_uuid AS "project_uuid!"
  FROM memory.search_events e
  JOIN sessions.sessions s ON s.id = e.session_id
  JOIN projects.projects p ON p.id = s.project_id
 WHERE e.tool = 'search_project_knowledge'
   AND e.used_final_at IS NOT NULL
   AND cardinality(e.used_references) > 0
   AND NOT EXISTS (SELECT 1 FROM memory.search_grades g WHERE g.event_id = e.id)
 ORDER BY e.id
 LIMIT $1
