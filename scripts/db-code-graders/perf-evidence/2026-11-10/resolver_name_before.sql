SELECT p.id, p.project_uuid, p.project_path
  FROM projects.projects p
 WHERE p.project_uuid = NULL::uuid OR p.project_path = 'mike'
UNION
SELECT p.id, p.project_uuid, p.project_path
  FROM projects.locations l
  JOIN projects.projects p ON p.id = l.project_id
 WHERE l.root_path = 'mike'
 ORDER BY 1
