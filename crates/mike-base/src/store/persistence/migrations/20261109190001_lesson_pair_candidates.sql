-- Pairs of lessons the reranker judged to be about the same thing, found by
-- the nightly rerank worker among pairs that embedding distance alone would
-- not nominate. `mike lesson-relations detect` judges these first. The table
-- is a queue for the relation detector: nothing here is a relation, and the
-- rerank worker never writes memory.lesson_relations.

CREATE TABLE IF NOT EXISTS memory.lesson_pair_candidates (
    lesson_a_id bigint           NOT NULL REFERENCES memory.lessons(id) ON DELETE CASCADE,
    lesson_b_id bigint           NOT NULL REFERENCES memory.lessons(id) ON DELETE CASCADE,
    score       real             NOT NULL,
    distance    double precision NOT NULL,
    found_at    timestamptz      NOT NULL DEFAULT now(),
    judged_at   timestamptz,
    PRIMARY KEY (lesson_a_id, lesson_b_id),
    CHECK (lesson_a_id < lesson_b_id)
);

CREATE INDEX IF NOT EXISTS idx_lesson_pair_candidates_unjudged
    ON memory.lesson_pair_candidates (judged_at) WHERE judged_at IS NULL;

COMMENT ON TABLE memory.lesson_pair_candidates IS
    'Queue of lesson pairs the reranker scored as near-duplicates, written nightly by the rerank worker and judged first by the lesson relation detector. Never a relation itself.';
COMMENT ON COLUMN memory.lesson_pair_candidates.lesson_a_id IS
    'The lesson with the smaller id of the pair.';
COMMENT ON COLUMN memory.lesson_pair_candidates.lesson_b_id IS
    'The lesson with the larger id of the pair.';
COMMENT ON COLUMN memory.lesson_pair_candidates.score IS
    'Reranker relevance of one lesson to the other, scored in both directions; the larger of the two.';
COMMENT ON COLUMN memory.lesson_pair_candidates.distance IS
    'Cosine distance between the two lessons'' embeddings when the pair was found.';
COMMENT ON COLUMN memory.lesson_pair_candidates.found_at IS
    'When the nightly pass last scored the pair.';
COMMENT ON COLUMN memory.lesson_pair_candidates.judged_at IS
    'When the relation detector handled the pair; null while it waits.';
