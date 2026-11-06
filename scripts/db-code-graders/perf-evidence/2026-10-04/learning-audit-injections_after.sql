SELECT p.project_path AS project,count(*) AS injections,
          count(DISTINCT i.lesson_id) AS lessons,count(DISTINCT i.session_id) AS sessions
          FROM memory.lesson_injections i JOIN sessions.sessions s ON s.id=i.session_id
          JOIN projects.projects p ON p.id=s.project_id
          WHERE i.injected_at >= '2026-09-27T17:40:00Z' AND i.injected_at < '2026-10-04T17:40:00Z' GROUP BY 1 ORDER BY 1