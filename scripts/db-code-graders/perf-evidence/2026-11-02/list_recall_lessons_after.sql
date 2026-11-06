WITH candidates AS (
    SELECT p.id, p.project_path, array_position(ARRAY['/mnt/data/projects/temet/temet-proxy','/mnt/data/projects/temet']::text[], p.project_path) AS rank
      FROM projects.projects p
     WHERE p.project_path = ANY(ARRAY['/mnt/data/projects/temet/temet-proxy','/mnt/data/projects/temet']::text[])
),
ranked AS (
    SELECT l.id, l.project_id, c.project_path, lk.code AS kind, l.text,
           l.agent_id, ls.code AS stage, l.source_turn_id, l.created_at, l.expires_at, l.topic,
           row_number() OVER (PARTITION BY l.topic ORDER BY c.rank, l.created_at DESC) AS position
      FROM candidates c
      JOIN memory.lessons l ON l.project_id = c.id
      JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
      JOIN memory.lesson_stages ls ON ls.id = l.stage_id
     WHERE l.topic = ANY(ARRAY['ui-design','db-ports']::text[])
       AND l.archived_at IS NULL
       AND (l.expires_at IS NULL OR l.expires_at > NOW())
)
SELECT id, project_id, project_path, kind, text, agent_id, stage, source_turn_id,
       created_at, expires_at, topic
  FROM ranked
 WHERE position <= 5
 ORDER BY topic, position
