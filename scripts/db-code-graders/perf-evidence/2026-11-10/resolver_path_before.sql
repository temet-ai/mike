SELECT p.id, p.project_uuid, p.project_path
  FROM projects.projects p
 WHERE p.project_uuid = NULL::uuid OR p.project_path = '/mnt/data/projects/temet/temet-proxy'
UNION
SELECT p.id, p.project_uuid, p.project_path
  FROM projects.locations l
  JOIN projects.projects p ON p.id = l.project_id
 WHERE l.root_path = '/mnt/data/projects/temet/temet-proxy'
 ORDER BY 1
