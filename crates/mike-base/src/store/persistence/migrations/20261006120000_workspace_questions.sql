-- A question one session asks another is a record with a state, not a mail.
-- It stays in front of the recipient until someone answers or declines it,
-- and a blocked one that nobody has settled is passed to the reference seat.
-- Everything here only adds; nothing existing changes.
--
-- to_session_id is NULL only while the question names a task whose seat has
-- not started; to_task keeps the name it was asked of. about_note_uuid is the
-- note (a brief part, a finding) the question is about.
--
-- escalated_at is the once-only mark for the reference-seat mail: it is set
-- by the statement that claims the question, so a restart cannot mail twice.

CREATE TABLE IF NOT EXISTS workspace.questions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    question_uuid uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    run_id text REFERENCES launch.runs(run_id) ON DELETE SET NULL,
    asker_session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    to_session_id bigint REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    to_task text,
    about_note_uuid uuid REFERENCES workspace.session_notes(note_uuid) ON DELETE SET NULL,
    question text NOT NULL CHECK (btrim(question) <> ''),
    blocked boolean NOT NULL DEFAULT false,
    state text NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'answered', 'declined')),
    answer text,
    settled_by_session_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    settled_at timestamptz,
    escalated_at timestamptz,
    CONSTRAINT questions_has_recipient CHECK (to_session_id IS NOT NULL OR to_task IS NOT NULL),
    CONSTRAINT questions_settled_matches_state CHECK ((state = 'open') = (settled_at IS NULL)),
    CONSTRAINT questions_open_has_no_answer CHECK (state <> 'open' OR answer IS NULL)
);

CREATE INDEX IF NOT EXISTS idx_questions_to_session_state
    ON workspace.questions (to_session_id, state);
CREATE INDEX IF NOT EXISTS idx_questions_run_state
    ON workspace.questions (run_id, state);
CREATE INDEX IF NOT EXISTS idx_questions_project_id
    ON workspace.questions (project_id);
CREATE INDEX IF NOT EXISTS idx_questions_asker_session_id
    ON workspace.questions (asker_session_id);
CREATE INDEX IF NOT EXISTS idx_questions_settled_by_session_id
    ON workspace.questions (settled_by_session_id) WHERE settled_by_session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_questions_about_note_uuid
    ON workspace.questions (about_note_uuid) WHERE about_note_uuid IS NOT NULL;

COMMENT ON TABLE workspace.questions IS
    'A question one session asked another: open until answered or declined. blocked means the asker waits on it; a blocked question open for five minutes is mailed once (escalated_at) to the fleet''s reference seat.';
