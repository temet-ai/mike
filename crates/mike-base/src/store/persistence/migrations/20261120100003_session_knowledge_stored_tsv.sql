-- Stored search vector for session learnings; see 20261120100000_knowledge_search_stored_tsv.sql.
ALTER TABLE memory.session_knowledge
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english'::regconfig, memory.session_learnings_text(learnings))) STORED;
CREATE INDEX idx_session_knowledge_search_tsv ON memory.session_knowledge USING gin (search_tsv);
DROP INDEX memory.idx_session_knowledge_learnings_text;
