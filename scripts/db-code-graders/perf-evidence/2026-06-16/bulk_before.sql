WITH incoming AS (
         SELECT n, p, d
           FROM UNNEST(
               ARRAY['rust-developer','db-dev','technical-writer','researcher','reviewer','architect']::TEXT[],
               ARRAY['/Users/vimes/projects/temet/temet-proxy/.claude/agents/rust-developer.md',
                     '/Users/vimes/projects/temet/temet-proxy/.claude/agents/db-dev.md',
                     '/Users/vimes/projects/temet/temet-proxy/.claude/agents/technical-writer.md',
                     '/Users/vimes/projects/temet/temet-proxy/.claude/agents/researcher.md',
                     '/Users/vimes/.claude/agents/reviewer.md',
                     '/Users/vimes/.claude/agents/architect.md']::TEXT[],
               ARRAY['desc-a','desc-b','desc-c','desc-d','desc-e','desc-f']::TEXT[]
           ) AS t(n, p, d)
     ),
     file_backed AS (
         SELECT id FROM agents.sources WHERE code = 'file_backed' LIMIT 1
     ),
     observed_source AS (
         SELECT id FROM agents.sources WHERE code = 'observed' LIMIT 1
     ),
     upd AS (
         UPDATE agents.agents a
            SET source_id = CASE
                    WHEN a.source_id = obs.id THEN fb.id
                    ELSE a.source_id
                END,
                definition_path = COALESCE(a.definition_path, i.p),
                description     = CASE
                    WHEN a.description IS NULL
                        THEN i.d
                    WHEN a.description ~ '^[>|][-+]?$'
                        THEN i.d
                    ELSE a.description
                END,
                updated_at      = NOW()
           FROM incoming i, file_backed fb, observed_source obs
          WHERE a.name = i.n
         RETURNING a.name
     ),
     ins AS (
         INSERT INTO agents.agents (name, definition_path, description, source_id)
         SELECT i.n, i.p, i.d, fb.id
           FROM incoming i, file_backed fb
          WHERE NOT EXISTS (
              SELECT 1 FROM agents.agents a WHERE a.name = i.n
          )
         RETURNING name
     )
     SELECT ((SELECT COUNT(*) FROM upd) + (SELECT COUNT(*) FROM ins))::BIGINT
