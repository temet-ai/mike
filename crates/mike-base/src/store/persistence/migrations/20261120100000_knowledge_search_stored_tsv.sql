-- The knowledge search re-derived to_tsvector(...) for every candidate of five kinds (lessons, questions, verdicts,
-- learnings, sessions) to rank and match it, about 140 ms of a 930 ms any-word pool call. Each kind now keeps its
-- vector in a column, indexed in place of the expression index over the same text, so matches and ranks are unchanged.
-- One table per migration file: each file is its own transaction, so it holds one table's lock at a time. Changing all
-- five in one transaction deadlocked against live writers on 2026-11-20.
CREATE EXTENSION IF NOT EXISTS btree_gin;

ALTER TABLE memory.lessons
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english'::regconfig, text)) STORED;
CREATE INDEX idx_lessons_search_tsv ON memory.lessons USING gin (search_tsv);
DROP INDEX memory.idx_lessons_text_tsv;
