-- The overnight grader now grades every real knowledge search, not only those whose result a
-- session used: each grade also holds the first ten results the search returned, with the
-- reranker's relevance of each to the query and to the prompt of the turn that asked it.
-- memory.search_tuning_passes records every tuner run, including a run that changed nothing
-- or found too few grades, and memory.search_tuning_runs rows point at their run.

ALTER TABLE memory.search_grades
    ADD COLUMN first_ten jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN turn_prompt text NULL,
    ADD COLUMN relevant_references text[] NOT NULL DEFAULT '{}'::text[],
    ADD CONSTRAINT search_grades_first_ten_is_array_check
        CHECK (jsonb_typeof(first_ten) = 'array');

COMMENT ON COLUMN memory.search_grades.first_ten IS 'JSON array of the first ten results the search returned, in the order returned: {reference, position, query_score|null, turn_score|null, relevant|null}. query_score is the reranker''s relevance to the query, turn_score its relevance to the query together with the asking turn''s prompt; relevant is true when either score reaches rerank.rescue_score, null when neither was scored.';
COMMENT ON COLUMN memory.search_grades.turn_prompt IS 'The user_prompt of the operator turn whose request made the search, as the grader read it; NULL when the request belongs to no turn.';
COMMENT ON COLUMN memory.search_grades.relevant_references IS 'The references of first_ten graded relevant. The tuner counts them, with used_references, as the results a good ranking puts in the top 5.';

CREATE TABLE memory.search_tuning_passes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ran_at timestamptz NOT NULL DEFAULT now(),
    outcome text NOT NULL,
    graded_events int NOT NULL,
    settings_tried int NOT NULL DEFAULT 0,
    settings_changed int NOT NULL DEFAULT 0,
    CONSTRAINT search_tuning_passes_outcome_check
        CHECK (outcome IN ('changed', 'no_change', 'too_few_grades')),
    CONSTRAINT search_tuning_passes_counts_check
        CHECK (graded_events >= 0 AND settings_tried >= 0
           AND settings_changed BETWEEN 0 AND settings_tried
           AND (outcome = 'changed') = (settings_changed > 0))
);

CREATE INDEX idx_search_tuning_passes_ran_at ON memory.search_tuning_passes (ran_at);

ALTER TABLE memory.search_tuning_runs
    ADD COLUMN pass_id bigint NULL REFERENCES memory.search_tuning_passes(id) ON DELETE CASCADE;

CREATE INDEX idx_search_tuning_runs_pass_id ON memory.search_tuning_runs (pass_id)
    WHERE pass_id IS NOT NULL;

WITH earlier AS (
    SELECT r.ran_at, max(r.graded_events) AS graded_events, count(*)::int AS tried,
           count(*) FILTER (WHERE r.kept)::int AS changed
      FROM memory.search_tuning_runs r
     WHERE r.pass_id IS NULL
     GROUP BY r.ran_at
), passes AS (
    INSERT INTO memory.search_tuning_passes (ran_at, outcome, graded_events, settings_tried, settings_changed)
    SELECT e.ran_at, CASE WHEN e.changed > 0 THEN 'changed' ELSE 'no_change' END,
           e.graded_events, e.tried, e.changed
      FROM earlier e
    RETURNING id, ran_at
)
UPDATE memory.search_tuning_runs r
   SET pass_id = p.id
  FROM passes p
 WHERE r.pass_id IS NULL
   AND r.ran_at = p.ran_at;

COMMENT ON TABLE memory.search_tuning_passes IS 'One row per run of the overnight ranking tuner: what it changed, that it found no change, or that there were too few graded searches to tune on. The per-setting detail is in memory.search_tuning_runs.';
COMMENT ON COLUMN memory.search_tuning_passes.ran_at IS 'When the run was recorded; tuned-today is any row since the start of the current quiet window.';
COMMENT ON COLUMN memory.search_tuning_passes.outcome IS 'changed: at least one setting was written; no_change: every candidate was rejected or none was tried; too_few_grades: fewer graded searches than rerank.tuning.min_graded_searches.';
COMMENT ON COLUMN memory.search_tuning_passes.graded_events IS 'Graded searches the run read.';
COMMENT ON COLUMN memory.search_tuning_passes.settings_tried IS 'Ranking settings the run tried; one memory.search_tuning_runs row each.';
COMMENT ON COLUMN memory.search_tuning_passes.settings_changed IS 'Settings the run wrote.';
COMMENT ON COLUMN memory.search_tuning_runs.pass_id IS 'FK -> memory.search_tuning_passes(id), the run this row belongs to. ON DELETE CASCADE.';
