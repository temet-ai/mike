WITH run_launches AS MATERIALIZED (
    SELECT DISTINCT ON (session_uuid)
           session_uuid, seat, identity ->> 'role' AS role
      FROM launch.launches
     WHERE run_id = $1 AND session_uuid IS NOT NULL
     ORDER BY session_uuid, created_at, id
), tagged_notes AS MATERIALIZED (
    SELECT n.id, n.session_id, f.project_path
      FROM launch.fleets f
      JOIN workspace.session_notes n
        ON n.tags @> ARRAY['fleet:' || f.fleet_uuid::text]
     WHERE f.run_id = $1
), note_ids AS (
    SELECT n.id
      FROM run_launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
      JOIN workspace.session_notes n ON n.session_id = s.id
     WHERE n.kind IN ('decision', 'task') OR n.status = 'operator'
    UNION
    SELECT n.id
      FROM tagged_notes n
      JOIN sessions.sessions s ON s.id = n.session_id
      JOIN projects.projects p ON p.id = s.project_id
                               AND p.project_path = n.project_path
)
SELECT n.note_uuid,
       s.session_uuid,
       n.kind,
       n.status,
       n.title,
       n.body,
       n.tags,
       n.data,
       n.created_at,
       n.updated_at,
       l.seat,
       l.role
  FROM note_ids i
  JOIN workspace.session_notes n ON n.id = i.id
  JOIN sessions.sessions s ON s.id = n.session_id
  LEFT JOIN run_launches l ON l.session_uuid = s.session_uuid
 ORDER BY n.updated_at, n.id
