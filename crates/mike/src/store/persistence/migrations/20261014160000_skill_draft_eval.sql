ALTER TABLE memory.skill_drafts ADD COLUMN eval_progress jsonb;

COMMENT ON COLUMN memory.skill_drafts.eval_progress IS
    'The eval loop''s partial work while the draft is evaluating: the sibling skills it was '
    'compared against, the models used, the trigger queries answered and the test prompts '
    'graded so far. A pass does as many of the remaining checks as the language-model call '
    'budget allows and saves them here, so a draft that needs more calls than one tick can '
    'make is finished over several ticks. Cleared when the result is stored in eval.';

CREATE TABLE memory.skill_draft_eval_failures (
    draft_id       bigint PRIMARY KEY REFERENCES memory.skill_drafts(id) ON DELETE CASCADE,
    attempts       integer NOT NULL DEFAULT 1,
    last_error     text NOT NULL,
    last_failed_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.skill_draft_eval_failures IS
    'Failed evaluation attempts for one skill draft: a model call failed or the grader '
    'returned an unusable answer. The eval loop waits 2^attempts hours after each failure and '
    'rejects the draft, with the last error in its eval column, after the third.';

ALTER TABLE memory.skill_drafts
    ADD COLUMN revision integer NOT NULL DEFAULT 1 CHECK (revision BETWEEN 1 AND 3);

ALTER TABLE memory.skill_drafts
    DROP CONSTRAINT skill_drafts_project_id_folder_lesson_watermark_key;

ALTER TABLE memory.skill_drafts
    ADD CONSTRAINT skill_drafts_topic_revision_key
    UNIQUE (project_id, folder, lesson_watermark, revision);

COMMENT ON COLUMN memory.skill_drafts.revision IS
    'Which attempt at this topic and lesson watermark the draft is. A rejected draft is redrafted with its evaluation as input, up to three drafts per watermark; a new lesson raises the watermark and starts again at 1.';
