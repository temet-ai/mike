-- Stored search vector for session overviews; see 20261120100000_knowledge_search_stored_tsv.sql.
-- sessions.sessions takes about 120 updates per inserted row, so its vector is kept by a trigger that fires only when
-- overview is written; a generated column would recompute it on every update.
ALTER TABLE sessions.sessions ADD COLUMN overview_tsv tsvector;

CREATE FUNCTION sessions.sessions_overview_tsv() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.overview_tsv := to_tsvector('english'::regconfig, NEW.overview);
    RETURN NEW;
END
$$;

CREATE TRIGGER sessions_overview_tsv BEFORE INSERT OR UPDATE OF overview ON sessions.sessions
    FOR EACH ROW EXECUTE FUNCTION sessions.sessions_overview_tsv();

UPDATE sessions.sessions SET overview_tsv = to_tsvector('english'::regconfig, overview) WHERE overview IS NOT NULL;

CREATE INDEX idx_sessions_overview_tsv ON sessions.sessions USING gin (overview_tsv);
DROP INDEX sessions.idx_sessions_overview_text;
