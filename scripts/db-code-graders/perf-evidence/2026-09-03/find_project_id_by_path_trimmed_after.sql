SELECT id, project_path FROM projects.projects
 WHERE project_path LIKE '/%'
   AND project_path LIKE '%/' || 'projects/temet/temet-proxy'
 ORDER BY last_seen DESC;
