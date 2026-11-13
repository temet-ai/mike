CREATE TABLE memory.skill_drafts (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id       bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    folder           text NOT NULL,
    agent_id         bigint REFERENCES agents.agents(id) ON DELETE SET NULL,
    lesson_watermark bigint NOT NULL,
    lesson_ids       bigint[] NOT NULL,
    model            text NOT NULL,
    description      text NOT NULL,
    skill_md         text NOT NULL,
    reference_files  jsonb NOT NULL DEFAULT '[]'::jsonb,
    test_prompts     jsonb NOT NULL DEFAULT '[]'::jsonb,
    assertions       jsonb NOT NULL DEFAULT '[]'::jsonb,
    trigger_queries  jsonb NOT NULL DEFAULT '[]'::jsonb,
    status           text NOT NULL DEFAULT 'draft'
                     CHECK (status IN ('draft', 'evaluating', 'accepted', 'rejected')),
    eval             jsonb,
    generation_id    text,
    cost_usd         double precision,
    created_at       timestamptz NOT NULL DEFAULT now(),
    evaluated_at     timestamptz,
    UNIQUE (project_id, folder, lesson_watermark)
);

CREATE INDEX skill_drafts_status_idx ON memory.skill_drafts (status, created_at);

COMMENT ON TABLE memory.skill_drafts IS
    'A skill drafted by the skill author pass from one topic''s accepted lessons, '
    'following the skill-creator method: a short SKILL.md that points to each file in '
    'reference_files ([{name, when_to_read, content}], written under references/), with the '
    'test prompts, assertions and trigger '
    'queries its evaluation needs. draft: written, not yet run. evaluating: the eval '
    'loop holds it. accepted: beat the baseline and triggers correctly; the promoter '
    'ships it as the folder''s SKILL.md. rejected: kept with its eval for the record.';

CREATE TABLE memory.skill_draft_failures (
    project_id       bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    folder           text NOT NULL,
    lesson_watermark bigint NOT NULL,
    attempts         integer NOT NULL DEFAULT 1,
    last_error       text NOT NULL,
    last_failed_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, folder, lesson_watermark)
);

COMMENT ON TABLE memory.skill_draft_failures IS
    'Failed skill author attempts for one topic at one lesson watermark: the model call '
    'failed, the draft was rejected or could not be written. The author pass waits 2^attempts '
    'hours after each failure and stops trying a topic after three, until a new lesson raises '
    'its watermark.';
