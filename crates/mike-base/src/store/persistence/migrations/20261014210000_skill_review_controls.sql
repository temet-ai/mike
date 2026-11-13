ALTER TABLE memory.skill_drafts
    ADD COLUMN review text CHECK (review IN ('approved', 'rejected')),
    ADD COLUMN reviewed_at timestamptz,
    ADD COLUMN edited_at timestamptz,
    ADD COLUMN authored_skill_md text;

COMMENT ON COLUMN memory.skill_drafts.review IS
    'The user''s decision on this draft from the Skills page: approved (ship it, even when the '
    'evaluator rejected it) or rejected (never ship this draft). NULL when the user has not '
    'decided. In a project set to ask, the promoter ships only approved drafts.';

COMMENT ON COLUMN memory.skill_drafts.authored_skill_md IS
    'The SKILL.md text as the skill author pass wrote it, kept the first time the user edits '
    'skill_md on the Skills page. NULL while the draft is unedited.';

CREATE TABLE memory.skill_policies (
    project_id bigint PRIMARY KEY REFERENCES projects.projects(id) ON DELETE CASCADE,
    mode       text NOT NULL CHECK (mode IN ('auto', 'ask')),
    changed_at timestamptz NOT NULL DEFAULT now(),
    changed_by text NOT NULL
);

COMMENT ON TABLE memory.skill_policies IS
    'Whether mike may write learned skills into a project''s checkouts on its own. auto: the '
    'promoter ships drafts that passed evaluation, or a generated skill when there is none. '
    'ask: it ships only drafts the user approved. A project with no row is ask, so a project '
    'first seen after this table was created never receives a skill unasked.';

INSERT INTO memory.skill_policies (project_id, mode, changed_by)
SELECT p.id, 'auto', 'existing project when skill review was added'
  FROM projects.projects p
 WHERE p.merged_into_id IS NULL;

CREATE TABLE memory.skill_retirements (
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    folder     text NOT NULL,
    retired_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, folder)
);

COMMENT ON TABLE memory.skill_retirements IS
    'Skill folders the user retired on the Skills page. The promoter removes them from every '
    'checkout and does not write them again until the user restores the folder, which deletes '
    'the row.';

CREATE TABLE memory.skill_installs (
    machine_uuid     uuid NOT NULL,
    project_id       bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    folder           text NOT NULL,
    machine_name     text NOT NULL,
    skill_path       text NOT NULL,
    draft_id         bigint REFERENCES memory.skill_drafts(id) ON DELETE SET NULL,
    lesson_watermark bigint NOT NULL,
    first_written    date NOT NULL,
    retired_at       date,
    retired_reason   text CHECK (retired_reason IN ('unloaded', 'user', 'policy')),
    reported_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (machine_uuid, project_id, folder)
);

CREATE INDEX skill_installs_project_idx ON memory.skill_installs (project_id, folder);

COMMENT ON TABLE memory.skill_installs IS
    'What the learning promoter last wrote into project checkouts on each machine, reported '
    'by the promoter at the end of every run: the SKILL.md path, the draft it came from (NULL '
    'for the generated skill), when it was first written and, once removed, when and why '
    '(unloaded: no load in 30 days; user: retired on the Skills page; policy: the project asks '
    'first and the user has not approved it).';
