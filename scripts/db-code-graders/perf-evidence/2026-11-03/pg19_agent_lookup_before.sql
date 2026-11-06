WITH upserted AS (
                INSERT INTO agents.agents (name, definition_path)
                VALUES ('function-reviewer', NULL::text)
                ON CONFLICT (LOWER(name)) DO UPDATE
                   SET name = EXCLUDED.name
                RETURNING id, name, definition_path, description, source_id,
                          created_at, updated_at
            )
            SELECT u.id AS "id!", u.name AS "name!", u.definition_path,
                   u.description, s.code AS "source!",
                   u.created_at AS "created_at!", u.updated_at AS "updated_at!"
              FROM upserted u
              JOIN agents.sources s ON s.id = u.source_id
