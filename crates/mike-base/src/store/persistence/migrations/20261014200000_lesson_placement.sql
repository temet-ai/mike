CREATE TABLE memory.lesson_placements (
    id          smallint PRIMARY KEY,
    code        text NOT NULL UNIQUE,
    description text NOT NULL
);

INSERT INTO memory.lesson_placements (id, code, description) VALUES
    (1, 'rule', 'A short standing constraint that applies whenever certain files are touched; belongs in .claude/rules and is proposed to the operator, never written by mike.'),
    (2, 'skill', 'A procedure for a recognisable situation, loaded on demand; the only placement that feeds skill drafts.'),
    (3, 'agent', 'A statement of who an agent is, what it owns or how it decides; belongs in the agent definition and is proposed to the operator, never written by mike.'),
    (4, 'memory', 'A fact or correction recalled when relevant; stays a lesson or a memory file.'),
    (5, 'nowhere', 'Already covered by an existing rule, skill, agent or memory, or not worth keeping.');

ALTER TABLE memory.lessons
    ADD COLUMN placement_id smallint REFERENCES memory.lesson_placements(id),
    ADD COLUMN placement_covered_by text,
    ADD COLUMN placement_reason text,
    ADD COLUMN placed_at timestamptz,
    ADD CONSTRAINT lessons_placement_consistent
        CHECK ((placement_id IS NULL) = (placed_at IS NULL));

CREATE INDEX idx_lessons_unplaced
    ON memory.lessons (id)
    WHERE placement_id IS NULL AND archived_at IS NULL;

CREATE TABLE memory.lesson_placement_failures (
    lesson_id      bigint PRIMARY KEY REFERENCES memory.lessons(id) ON DELETE CASCADE,
    attempts       integer NOT NULL DEFAULT 1,
    last_error     text NOT NULL,
    last_failed_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.lesson_placement_failures IS
    'Failed placement attempts for one lesson: the model call failed or its answer could not be stored. The placement pass waits 2^attempts hours after each failure and stops trying the lesson after the third; the lesson stays unplaced and is counted as unplaced in the weekly learning audit.';

COMMENT ON COLUMN memory.lessons.placement_id IS
    'Where the lesson belongs, decided by the language model when the lesson is written (the same call that decides store, skip, update or merge) or by the placement pass on the lesson proposer tick for lessons written without one. NULL means not placed yet. Only lessons placed as skill feed skill drafts; rule and agent placements are listed for the operator and never written into a skill.';

COMMENT ON COLUMN memory.lessons.placement_covered_by IS
    'The memory file or skill, as the proposer was shown it, that already covers the lesson or that the lesson belongs in. Only a document that was shown to the model is stored.';

COMMENT ON COLUMN memory.lessons.placement_reason IS
    'One sentence from the model saying why the lesson has this placement.';

COMMENT ON COLUMN memory.lessons.placed_at IS
    'When the placement was stored.';

COMMENT ON TABLE memory.lesson_placements IS
    'The homes a learned lesson can have: rule, skill, agent, memory or nowhere.';

ALTER TABLE memory.lesson_proposal_guidance
    DROP CONSTRAINT lesson_proposal_guidance_source_kind_check;

ALTER TABLE memory.lesson_proposal_guidance
    ADD CONSTRAINT lesson_proposal_guidance_source_kind_check
    CHECK (source_kind IN ('memory', 'skill', 'rule', 'agent'));

ALTER TABLE memory.skill_drafts ADD COLUMN topic text;

UPDATE memory.skill_drafts d
   SET topic = l.topic
  FROM memory.lessons l
 WHERE l.id = d.lesson_ids[1];

COMMENT ON COLUMN memory.skill_drafts.topic IS
    'The lesson topic the draft was written for. The skill folder is the topic, or the topic with the agent name when the lessons are scoped to an agent. Used to find an existing skill with the same purpose, so a new topic that overlaps it patches that skill instead of adding another.';
