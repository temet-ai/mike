SELECT s.session_uuid::text, p.project_path FROM sessions.sessions s
JOIN projects.projects p ON p.id = s.project_id
WHERE s.location_uuid IS NOT NULL OR s.project_resolution = 'operator';
