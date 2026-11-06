UPDATE sessions.sessions
SET project_id = 182658
WHERE id = 536896
  AND project_id = (
      SELECT id FROM projects.projects
      WHERE project_path = '(no project)'
      LIMIT 1
  );
