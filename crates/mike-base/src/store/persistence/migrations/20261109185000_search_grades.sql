-- memory.search_grades holds the overnight grade of one recorded knowledge search: the wider candidate
-- list the search would have drawn from, with the reranker's score for each, and where the references
-- the session went on to use sat in the plain order and after reranking. memory.search_tuning_runs
-- records every ranking-setting change the overnight tuner tried, kept or not.

CREATE TABLE memory.search_grades (
    event_id bigint PRIMARY KEY REFERENCES memory.search_events(id) ON DELETE CASCADE,
    graded_at timestamptz NOT NULL DEFAULT now(),
    candidates jsonb NOT NULL,
    used_references text[] NOT NULL,
    plain_positions int[] NOT NULL,
    reranked_positions int[] NOT NULL,
    CONSTRAINT search_grades_candidates_is_array_check
        CHECK (jsonb_typeof(candidates) = 'array'),
    CONSTRAINT search_grades_positions_match_used_check
        CHECK (cardinality(plain_positions) = cardinality(used_references)
           AND cardinality(reranked_positions) = cardinality(used_references))
);

CREATE INDEX idx_search_grades_graded_at ON memory.search_grades (graded_at);

CREATE TABLE memory.search_tuning_runs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ran_at timestamptz NOT NULL DEFAULT now(),
    setting_key text NOT NULL,
    before jsonb NOT NULL,
    after jsonb NOT NULL,
    score_before double precision NOT NULL,
    score_after double precision NOT NULL,
    graded_events int NOT NULL,
    kept boolean NOT NULL,
    reason text NOT NULL
);

CREATE INDEX idx_search_tuning_runs_ran_at ON memory.search_tuning_runs (ran_at);

COMMENT ON TABLE memory.search_grades IS 'One row per recorded search_project_knowledge call that the rerank_nightly worker has graded: the plain candidate list (up to 200, in plain order) with each candidate''s reranker score, and the positions of the references the session used. Written once per event; deleted with its event.';
COMMENT ON COLUMN memory.search_grades.event_id IS 'PK and FK -> memory.search_events(id) of the graded search. ON DELETE CASCADE.';
COMMENT ON COLUMN memory.search_grades.graded_at IS 'When the rerank_nightly worker graded the event; the tuner reads the grades of the last 14 days.';
COMMENT ON COLUMN memory.search_grades.candidates IS 'JSON array in plain order: {reference, kind, lexical, semantic, rerank_score|null}. lexical and semantic are the search row''s lexical_rank and semantic_rank; rerank_score is null when the reranker did not score the candidate.';
COMMENT ON COLUMN memory.search_grades.used_references IS 'Copy of memory.search_events.used_references at grading time.';
COMMENT ON COLUMN memory.search_grades.plain_positions IS 'For each used reference, in the same order, its 1-based position in the plain candidate order; 0 when it is not among the candidates.';
COMMENT ON COLUMN memory.search_grades.reranked_positions IS 'Same as plain_positions after the top rerank.candidates candidates are reordered by rerank_score (unscored candidates keep their order after scored ones).';

COMMENT ON TABLE memory.search_tuning_runs IS 'One row per ranking setting the overnight tuner tried in a run: the kept change, or the best rejected candidate. A kept change is also written as a setting with the source rerank_tuner.';
COMMENT ON COLUMN memory.search_tuning_runs.ran_at IS 'When the tuning run recorded the row; tuned-today is any row since the start of the current quiet window.';
COMMENT ON COLUMN memory.search_tuning_runs.setting_key IS 'The settings key tried, one of the search.ranking.* keys.';
COMMENT ON COLUMN memory.search_tuning_runs.before IS 'JSON value of the setting before the change.';
COMMENT ON COLUMN memory.search_tuning_runs.after IS 'JSON value of the candidate: the value kept, or the best rejected value.';
COMMENT ON COLUMN memory.search_tuning_runs.score_before IS 'Share of used references in the top 5 over the graded searches with the value before.';
COMMENT ON COLUMN memory.search_tuning_runs.score_after IS 'The same share with the candidate value.';
COMMENT ON COLUMN memory.search_tuning_runs.graded_events IS 'How many graded searches the shares were measured over.';
COMMENT ON COLUMN memory.search_tuning_runs.kept IS 'True when the candidate improved the overall share by at least 0.01 with no kind dropping and was written as a setting.';
COMMENT ON COLUMN memory.search_tuning_runs.reason IS 'Why the candidate was kept or rejected, in words.';
