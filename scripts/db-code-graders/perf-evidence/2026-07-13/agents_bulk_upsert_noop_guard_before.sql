WITH incoming AS (
    SELECT n, p, d
      FROM UNNEST(
        ARRAY['db-dev','rust-developer','technical-writer']::TEXT[],
        ARRAY['/Users/vimes/projects/temet/temet-proxy/.claude/agents/db-dev.md','/Users/vimes/projects/temet/temet-proxy/.claude/agents/rust-developer.md','/Users/vimes/projects/temet/temet-proxy/.claude/agents/technical-writer.md']::TEXT[],
        ARRAY['Designs and implements Postgres schemas, migrations, indexes, and sqlx query architecture for the claude-proxy persistence layer.','Implements production-ready Rust code for the claude-proxy workspace.','Maintains project documentation and rustdoc clarity for temet-proxy without touching production logic.']::TEXT[]
      ) AS t(n, p, d)
),
file_backed AS (
    SELECT id FROM agents.sources WHERE code = 'file_backed' LIMIT 1
),
upserted AS (
    INSERT INTO agents.agents (name, definition_path, description, source_id)
    SELECT i.n, i.p, i.d, fb.id
      FROM incoming i, file_backed fb
    ON CONFLICT (LOWER(name)) DO UPDATE
       SET source_id       = (SELECT fb2.id FROM agents.sources fb2
                              WHERE fb2.code = 'file_backed' LIMIT 1),
           definition_path = COALESCE(agents.agents.definition_path, EXCLUDED.definition_path),
           description     = CASE
               WHEN agents.agents.description IS NULL
                   THEN EXCLUDED.description
               WHEN agents.agents.description ~ '^[>|][-+]?$'
                   THEN EXCLUDED.description
               ELSE agents.agents.description
           END,
           updated_at      = NOW()
    RETURNING name
)
SELECT COUNT(*)::BIGINT FROM upserted
