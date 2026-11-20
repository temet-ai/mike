-- The knowledge search re-derived to_tsvector(...) for every candidate of five kinds (lessons, questions, verdicts,
-- learnings, sessions) to rank and match it, about 140 ms of a 930 ms any-word pool call. Each kind now keeps its
-- vector in a column, indexed in place of the expression index over the same text, so a candidate reads the stored
-- vector. The expressions are the ones the expression indexes used, so matches and ranks are unchanged.
-- sessions.sessions takes about 120 updates per inserted row, so its vector is kept by a trigger that fires only when
-- overview is written; a generated column would recompute it on every update.

CREATE EXTENSION IF NOT EXISTS btree_gin;

ALTER TABLE memory.lessons
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english'::regconfig, text)) STORED;
CREATE INDEX idx_lessons_search_tsv ON memory.lessons USING gin (search_tsv);
DROP INDEX memory.idx_lessons_text_tsv;

ALTER TABLE workspace.questions
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english'::regconfig, question || ' ' || COALESCE(answer, ''))) STORED;
CREATE INDEX idx_questions_search_tsv ON workspace.questions USING gin (search_tsv);
DROP INDEX workspace.idx_questions_text;

ALTER TABLE memory.turn_verdicts
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english'::regconfig, unmet_ask || ' ' || rationale)) STORED;
CREATE INDEX idx_turn_verdicts_live_search_tsv ON memory.turn_verdicts USING gin (search_tsv) WHERE mode = 'live';
DROP INDEX memory.idx_turn_verdicts_live_text;

CREATE OR REPLACE VIEW memory.live_turn_verdicts_v AS
SELECT v.id, v.operator_turn_id, v.session_id, v.reaction, v.unmet_ask, v.rationale, v.created_at, t.project_id,
       v.search_tsv
  FROM memory.turn_verdicts v
  JOIN memory.operator_turns t ON t.id = v.operator_turn_id
 WHERE v.mode = 'live'
   AND NOT EXISTS (SELECT 1 FROM memory.turn_verdicts n
                    WHERE n.operator_turn_id = v.operator_turn_id AND n.mode = 'live'
                      AND (n.created_at, n.id) > (v.created_at, v.id));

ALTER TABLE memory.session_knowledge
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english'::regconfig, memory.session_learnings_text(learnings))) STORED;
CREATE INDEX idx_session_knowledge_search_tsv ON memory.session_knowledge USING gin (search_tsv);
DROP INDEX memory.idx_session_knowledge_learnings_text;

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
