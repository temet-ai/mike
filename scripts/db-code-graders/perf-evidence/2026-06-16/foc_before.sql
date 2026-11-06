WITH existing AS (
        SELECT a.id, a.name, a.definition_path, a.description,
               s.code AS source_code, a.created_at, a.updated_at
          FROM agents.agents a
          JOIN agents.sources s ON s.id = a.source_id
         WHERE a.name = 'rust-developer'
         ORDER BY a.updated_at DESC LIMIT 1
    ),
    ins AS (
        INSERT INTO agents.agents (name, definition_path)
        SELECT 'rust-developer', '/Users/vimes/projects/temet/temet-proxy/.claude/agents/rust-developer.md'
         WHERE NOT EXISTS (SELECT 1 FROM existing)
        RETURNING id, name, definition_path, description, source_id,
                  created_at, updated_at
    ),
    ins_full AS (
        SELECT i.id, i.name, i.definition_path, i.description,
               s.code AS source_code, i.created_at, i.updated_at
          FROM ins i
          JOIN agents.sources s ON s.id = i.source_id
    )
    SELECT id AS "id", name AS "name", definition_path, description,
           source_code AS "source", created_at AS "created_at",
           updated_at AS "updated_at"
      FROM (
          SELECT id, name, definition_path, description,
                 source_code, created_at, updated_at FROM ins_full
          UNION ALL
          SELECT id, name, definition_path, description,
                 source_code, created_at, updated_at FROM existing
          LIMIT 1
      ) picked
