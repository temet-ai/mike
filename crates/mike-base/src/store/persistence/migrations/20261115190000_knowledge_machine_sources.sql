-- A knowledge source may belong to a machine instead of a project location (the machine's ~/.claude agents, rules,
-- skills and agent memory). Its documents have no project: knowledge_body_artifact_v gives their items project_id NULL
-- and every project's search, read and guidance include them beside the project's own documents.
--
-- An artifact body is embedded once. A document's identity for sharing is (scope, source name, relative_path), where
-- scope is the location's project for a location source and 'machine' for a machine source. When the sweep picks an
-- item whose current revision has a twin (another current document of the same identity with the same content_hash)
-- already embedded for the model, memory.share_twin_embeddings copies the twin's chunks instead of calling the embedder.

ALTER TABLE memory.knowledge_sources
    ADD COLUMN IF NOT EXISTS machine_uuid uuid REFERENCES projects.machines (machine_uuid) ON DELETE SET NULL,
    ALTER COLUMN location_uuid DROP NOT NULL;

-- A machine source loses its machine_uuid when that machine identity is merged into another and deleted; the
-- scanner of the surviving machine adopts it by name and root path, so its documents and embeddings carry over.
-- Until then it has no owner and search leaves it out.
ALTER TABLE memory.knowledge_sources
    DROP CONSTRAINT IF EXISTS knowledge_sources_owner_check,
    ADD CONSTRAINT knowledge_sources_owner_check CHECK (location_uuid IS NULL OR machine_uuid IS NULL);

CREATE UNIQUE INDEX IF NOT EXISTS knowledge_sources_machine_name_key
    ON memory.knowledge_sources (machine_uuid, name) WHERE machine_uuid IS NOT NULL;

COMMENT ON COLUMN memory.knowledge_sources.machine_uuid IS
    'Set for a source that belongs to a machine and no project location; location_uuid is then NULL. Exactly one of the two is set.';

CREATE OR REPLACE VIEW memory.knowledge_body_artifact_v AS
SELECT r.id AS source_id, loc.project_id, 'artifact:' || r.id AS reference,
       CASE src.name WHEN 'claude-memory' THEN 'memory' WHEN 'skills' THEN 'skill' ELSE 'artifact' END AS label,
       r.title AS name, r.title || E'\n\n' || r.body AS body
  FROM memory.knowledge_documents d
  JOIN memory.knowledge_revisions r ON r.id = d.current_revision
  JOIN memory.knowledge_sources src ON src.source_uuid = d.source_uuid
  LEFT JOIN projects.locations loc ON loc.location_uuid = src.location_uuid
 WHERE d.deleted_at IS NULL;

CREATE OR REPLACE FUNCTION memory.share_twin_embeddings(p_item_ids bigint[], p_model_key text)
RETURNS bigint[]
LANGUAGE plpgsql AS $$
DECLARE
    v_items bigint[];
    v_twins bigint[];
    v_served bigint[];
BEGIN
    SELECT array_agg(t.item_id ORDER BY t.item_id), array_agg(t.twin_state_id ORDER BY t.item_id)
      INTO v_items, v_twins
      FROM (
        SELECT DISTINCT ON (m.item_id) m.item_id, twin.id AS twin_state_id
          FROM (
            SELECT i.id AS item_id, d.document_uuid, d.relative_path, s.name AS source_name,
                   s.machine_uuid IS NOT NULL AS machine_scope, loc.project_id AS scope_project,
                   r.title, r.content_hash
              FROM memory.knowledge_items i
              JOIN memory.knowledge_revisions r ON r.id = i.source_id
              JOIN memory.knowledge_documents d
                ON d.document_uuid = r.document_uuid AND d.current_revision = r.id AND d.deleted_at IS NULL
              JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
              LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
             WHERE i.id = ANY(p_item_ids) AND i.kind = 'artifact' AND i.needs_embedding
          ) m
          JOIN memory.knowledge_sources s2 ON s2.name = m.source_name
          LEFT JOIN projects.locations loc2 ON loc2.location_uuid = s2.location_uuid
          JOIN memory.knowledge_documents d2
            ON d2.source_uuid = s2.source_uuid AND d2.relative_path = m.relative_path
           AND d2.deleted_at IS NULL AND d2.document_uuid <> m.document_uuid
          JOIN memory.knowledge_revisions r2
            ON r2.id = d2.current_revision AND r2.title = m.title AND r2.content_hash = m.content_hash
          JOIN memory.knowledge_embedding_state twin
            ON twin.source_kind = 'artifact' AND twin.source_id = r2.id
           AND twin.model_key = p_model_key AND twin.completed_at IS NOT NULL
         WHERE (m.machine_scope AND s2.machine_uuid IS NOT NULL)
            OR (NOT m.machine_scope AND loc2.project_id = m.scope_project)
         ORDER BY m.item_id, twin.id
      ) t;
    IF v_items IS NULL THEN
        RETURN ARRAY[]::bigint[];
    END IF;
    DELETE FROM memory.knowledge_embedding_chunks c
     USING memory.knowledge_embedding_state e
     WHERE e.item_id = ANY(v_items) AND c.state_id = e.id;
    WITH served AS (
        SELECT u.item_id, u.twin_state_id FROM unnest(v_items, v_twins) AS u(item_id, twin_state_id)
    ), written AS (
        INSERT INTO memory.knowledge_embedding_state
            (item_id, source_kind, source_id, reference, project_id, model_key, completed_at, name, label)
        SELECT i.id, 'artifact', i.source_id, b.reference, b.project_id, p_model_key, now(), b.name, b.label
          FROM served s
          JOIN memory.knowledge_items i ON i.id = s.item_id
          JOIN memory.knowledge_body_artifact_v b ON b.source_id = i.source_id
        ON CONFLICT (item_id) DO UPDATE SET
            reference = EXCLUDED.reference, project_id = EXCLUDED.project_id, model_key = EXCLUDED.model_key,
            completed_at = now(), attempts = 0, last_error = NULL, name = EXCLUDED.name, label = EXCLUDED.label,
            search_tsv = NULL
        RETURNING id, item_id
    ), copied AS (
        INSERT INTO memory.knowledge_embedding_chunks (state_id, ordinal, body, embedding)
        SELECT w.id, c.ordinal, c.body, c.embedding
          FROM written w
          JOIN served s ON s.item_id = w.item_id
          JOIN memory.knowledge_embedding_chunks c ON c.state_id = s.twin_state_id
        RETURNING 1
    )
    SELECT array_agg(w.item_id ORDER BY w.item_id) INTO v_served FROM written w;
    IF v_served IS NULL THEN
        RETURN ARRAY[]::bigint[];
    END IF;
    UPDATE memory.knowledge_items i
       SET needs_embedding = false, attempts = 0, last_error = NULL, model_key = p_model_key
     WHERE i.id = ANY(v_served);
    RETURN v_served;
END
$$;

COMMENT ON FUNCTION memory.share_twin_embeddings(bigint[], text) IS
    'Serves artifact items that still need embedding from a twin: another current document with the same scope (project, or machine), source name and relative_path whose current revision has the same title and content_hash and is already embedded for p_model_key. Copies the twin''s chunks into the item''s own state row and marks the item embedded. Returns the item ids served; the caller embeds the rest.';

SELECT memory.share_twin_embeddings(
           (SELECT array_agg(i.id) FROM memory.knowledge_items i WHERE i.kind = 'artifact' AND i.needs_embedding),
           m.model_key)
  FROM (SELECT e.model_key
          FROM memory.knowledge_embedding_state e
         WHERE e.source_kind = 'artifact' AND e.completed_at IS NOT NULL
         GROUP BY e.model_key
         ORDER BY count(*) DESC, e.model_key
         LIMIT 1) m;
