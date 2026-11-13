-- The knowledge search read a text vector built from a work thread's title, slug and goal, and another built from its
-- state fields, for every thread of the project on every call. Both are now stored generated columns, so the search
-- reads them and nothing can go stale. A generated column rejects concat_ws (only STABLE), so the texts are joined with
-- || and COALESCE; the added separators are whitespace, so the words and positions are the same. Both tables are small
-- (a few hundred rows), so the rewrite that adding a stored column causes takes milliseconds.
ALTER TABLE work.threads
    ADD COLUMN IF NOT EXISTS search_tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('english', COALESCE(title, '') || E'\n' || COALESCE(slug, '') || E'\n'
                                                || COALESCE(goal, ''))) STORED;

ALTER TABLE work.thread_state
    ADD COLUMN IF NOT EXISTS search_tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('english', COALESCE(tldr, '') || E'\n' || COALESCE(where_we_are, '') || E'\n'
                                                || COALESCE(next_up, '') || E'\n' || COALESCE(blockers, '') || E'\n'
                                                || COALESCE(open_questions, '') || E'\n'
                                                || COALESCE(decisions, ''))) STORED;
