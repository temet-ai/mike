UPDATE sessions.sessions
 SET project_id  = CASE
     WHEN project_id = (
         SELECT id FROM projects.projects
         WHERE project_path = '(no project)'
         LIMIT 1
     ) THEN 1
     ELSE project_id
 END,
     last_seen   = GREATEST(last_seen, NOW()),
     last_status = 'Pending'
 WHERE id = 574994
