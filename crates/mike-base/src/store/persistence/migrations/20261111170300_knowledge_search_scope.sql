-- Knowledge search built the list of a project's embedded items by joining every row of
-- memory.knowledge_embedding_state to memory.knowledge_sources_with_sessions_v (issue #151). Each state row now
-- carries the project and the reference of its source, filled by a trigger from that view whenever the row is
-- written, so the search reads one project's rows through an index.

ALTER TABLE memory.knowledge_embedding_state
    ADD COLUMN project_id bigint,
    ADD COLUMN reference text;

COMMENT ON COLUMN memory.knowledge_embedding_state.project_id IS
    'Project of the source when the row was last written, from memory.knowledge_sources_with_sessions_v; NULL when the source is gone. A source that moved project is embedded again by the sweep, which rewrites it.';
COMMENT ON COLUMN memory.knowledge_embedding_state.reference IS
    'Search reference of the source (kind:id or kind:uuid), from memory.knowledge_sources_with_sessions_v.';

UPDATE memory.knowledge_embedding_state e
   SET project_id = src.project_id, reference = src.reference
  FROM memory.knowledge_sources_with_sessions_v src
 WHERE src.source_kind = e.source_kind AND src.source_id = e.source_id;

CREATE FUNCTION memory.fill_knowledge_embedding_scope() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    SELECT src.project_id, src.reference INTO NEW.project_id, NEW.reference
      FROM memory.knowledge_sources_with_sessions_v src
     WHERE src.source_kind = NEW.source_kind AND src.source_id = NEW.source_id
     LIMIT 1;
    RETURN NEW;
END
$$;

CREATE TRIGGER trg_knowledge_embedding_state_scope
    BEFORE INSERT OR UPDATE OF source_kind, source_id, source_updated_at, source_seq, source_hash,
                               model_key, completed_at
    ON memory.knowledge_embedding_state
    FOR EACH ROW EXECUTE FUNCTION memory.fill_knowledge_embedding_scope();

CREATE INDEX idx_knowledge_embedding_state_project
    ON memory.knowledge_embedding_state (project_id, model_key) INCLUDE (reference)
    WHERE completed_at IS NOT NULL;
