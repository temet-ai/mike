-- Reranked candidates for a work thread's card: the lessons, thread entries,
-- session notes and operator turns of the thread's project that the reranker
-- judged closest to the thread's title, goal and latest checkpoint. The
-- nightly rerank worker rewrites a thread's rows in one transaction; the card
-- worker reads the best few that are still live and renders them as the
-- card's Related section.
--
-- Derived data: safe to delete, rebuilt by the next nightly pass. A write
-- bumps the thread_card_sources change counter (memory.change_generations),
-- so the card worker looks at the thread again; the table never touches
-- work.thread_cards itself.

CREATE TABLE IF NOT EXISTS work.thread_card_candidates (
    thread_id bigint      NOT NULL REFERENCES work.threads(id) ON DELETE CASCADE,
    rank      int         NOT NULL,
    ref       text        NOT NULL,
    kind      text        NOT NULL,
    score     real        NOT NULL,
    built_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (thread_id, rank)
);

COMMENT ON TABLE work.thread_card_candidates IS
    'Reranked knowledge candidates per work thread, rewritten nightly by the rerank worker (thread_id, rank 1..n) and read by memory-keeper for the card''s Related section. Derived data: safe to delete.';
COMMENT ON COLUMN work.thread_card_candidates.thread_id IS
    'The work thread the candidate relates to. Rows go with the thread.';
COMMENT ON COLUMN work.thread_card_candidates.rank IS
    'Position among the thread''s candidates, 1 for the highest reranker score.';
COMMENT ON COLUMN work.thread_card_candidates.ref IS
    'Reference token of the candidate: lesson:<uuid>, entry:<thread_log id>, note:<uuid> or turn:<operator turn id>.';
COMMENT ON COLUMN work.thread_card_candidates.kind IS
    'Kind of the candidate, the prefix of ref: lesson, entry, note or turn.';
COMMENT ON COLUMN work.thread_card_candidates.score IS
    'Reranker relevance of the candidate to the thread''s query text.';
COMMENT ON COLUMN work.thread_card_candidates.built_at IS
    'When the nightly pass wrote the row.';

DROP TRIGGER IF EXISTS trg_thread_card_candidates_card_sources ON work.thread_card_candidates;
CREATE TRIGGER trg_thread_card_candidates_card_sources
    AFTER INSERT OR DELETE ON work.thread_card_candidates
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');
