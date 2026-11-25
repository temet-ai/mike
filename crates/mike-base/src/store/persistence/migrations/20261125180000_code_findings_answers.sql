-- mike-code: a session answers a finding it will not fix.
--
-- answer is the state of that answer: kept (the reason was accepted), not_mine
-- (mike held no proof the session wrote the line), refused (the reason was not
-- good enough; the finding stays open) or waiting (the person rules: the
-- finding is an error, the session used up its keeps or refusals, or no judge
-- answered). outcome gains kept and not_mine, both closed.
-- reminded_at is the one reminder sent when the path was committed unanswered.

ALTER TABLE workspace.code_findings
    ADD COLUMN IF NOT EXISTS answer text,
    ADD COLUMN IF NOT EXISTS answer_reason text,
    ADD COLUMN IF NOT EXISTS answer_ruling text,
    ADD COLUMN IF NOT EXISTS answered_at timestamptz,
    ADD COLUMN IF NOT EXISTS answer_refusals integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS reminded_at timestamptz;

ALTER TABLE workspace.code_findings
    DROP CONSTRAINT IF EXISTS code_findings_outcome_check,
    ADD CONSTRAINT code_findings_outcome_check
        CHECK (outcome IN ('open', 'fixed', 'fixed_by_other', 'ignored', 'superseded', 'kept', 'not_mine')),
    ADD CONSTRAINT code_findings_answer_check
        CHECK (answer IS NULL OR answer IN ('kept', 'not_mine', 'refused', 'waiting'));

CREATE INDEX IF NOT EXISTS idx_code_findings_waiting
    ON workspace.code_findings (answered_at) WHERE answer = 'waiting';
