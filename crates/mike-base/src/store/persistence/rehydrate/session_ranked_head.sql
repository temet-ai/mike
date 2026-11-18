WITH ranked AS (
    SELECT s.id,
           s.session_uuid::text AS session_id,
           p.project_path,
           CASE WHEN s.project_resolution = 'unresolved' OR p.project_path = '(no project)' THEN NULL ELSE p.id END AS project_id,
           s.cwd,
           s.project_resolution,
           s.first_seen,
           s.last_seen,
           s.last_status,
           s.title,
           s.pinned,
           p.pinned AS project_pinned,
           s.overview,
           s.overview_generated_at,
           s.machine_uuid,
           sm.name AS machine_name
    FROM sessions.sessions s
    JOIN projects.projects p ON p.id = s.project_id
    LEFT JOIN projects.machines sm ON sm.machine_uuid = s.machine_uuid
