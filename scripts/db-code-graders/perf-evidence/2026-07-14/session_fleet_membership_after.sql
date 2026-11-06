WITH RECURSIVE peer_edges AS (
    SELECT m.to_session_id AS peer_id
      FROM mailbox.messages m
     WHERE m.from_session_id = 480448 AND m.to_session_id <> 480448
    UNION ALL
    SELECT m.from_session_id AS peer_id
      FROM mailbox.messages m
     WHERE m.to_session_id = 480448
       AND m.from_session_id IS NOT NULL
       AND m.from_session_id <> 480448
),
edge_totals AS (
    SELECT COUNT(*) AS message_count,
           COUNT(DISTINCT peer_id) AS peer_count
      FROM peer_edges
),
seed AS (
    SELECT id, project_id FROM sessions.sessions WHERE id = 480448
),
project_edges AS (
    SELECT m.from_session_id AS a, m.to_session_id AS b
      FROM mailbox.messages m
      JOIN sessions.sessions sf ON sf.id = m.from_session_id
      JOIN sessions.sessions st ON st.id = m.to_session_id
      CROSS JOIN seed
     WHERE m.from_session_id IS NOT NULL
       AND m.from_session_id <> m.to_session_id
       AND sf.project_id = seed.project_id
       AND st.project_id = seed.project_id
),
component(session_id) AS (
    SELECT id FROM seed
    UNION
    SELECT CASE WHEN pe.a = c.session_id THEN pe.b ELSE pe.a END
      FROM project_edges pe
      JOIN component c ON c.session_id IN (pe.a, pe.b)
),
component_totals AS (
    SELECT COUNT(*) AS fleet_size,
           MIN(s.last_seen) AS oldest_last_seen
      FROM component c
      JOIN sessions.sessions s ON s.id = c.session_id
)
SELECT
    p.project_path        AS project_path,
    et.message_count       AS message_count,
    et.peer_count          AS peer_count,
    ct.fleet_size          AS fleet_size,
    ct.oldest_last_seen    AS oldest_last_seen
FROM sessions.sessions s
LEFT JOIN projects.projects p ON p.id = s.project_id
CROSS JOIN edge_totals et
CROSS JOIN component_totals ct
WHERE s.id = 480448;
