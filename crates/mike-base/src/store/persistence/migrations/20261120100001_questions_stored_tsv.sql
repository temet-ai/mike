-- Stored search vector for questions; see 20261120100000_knowledge_search_stored_tsv.sql.
ALTER TABLE workspace.questions
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english'::regconfig, question || ' ' || COALESCE(answer, ''))) STORED;
CREATE INDEX idx_questions_search_tsv ON workspace.questions USING gin (search_tsv);
DROP INDEX workspace.idx_questions_text;
