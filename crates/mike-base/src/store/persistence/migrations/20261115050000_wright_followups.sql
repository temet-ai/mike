-- Wright follow-up register (temet-ai/mike#269): what an engineer run finds that is not a fix
-- for this one problem, kept so it is filed once, handed to a fixer and followed to the end.
--
-- wright.followups: one row per piece of follow-up work. kind says what it is (check_tuning,
-- code_defect, hardening, or root_cause: why a remedy keeps closing problems of one check).
-- check_name and owning_part say where it belongs; the partial unique index keeps one live
-- follow-up per kind, check and part, so a second finding on the same thing becomes more
-- evidence on the open one. States: open (not yet filed), filed (a GitHub issue exists,
-- issue_number), handed_off (a fixer session works it; fixer_session and fixer_machine),
-- fixed and dropped (closed_at, closed_reason), stuck (the fixer ended max_attempts times
-- without finishing). attempts counts fixer sessions that ended without finishing.
-- wright.followup_findings: the findings that evidence a follow-up. A root_cause follow-up
-- opened from recurring remedies has none until its engineer run finishes.
-- wright.investigations.followup_id: the follow-up an engineer run was started for.
-- wright.followup_limits: one row. Daily filing and hand-off budgets, fixer attempts, when a
-- remedy counts as recurring, the repository issues go to, and the machines a fixer may run on
-- (an array of {machine, host, folder}; empty means the local checkout). Changed only through
-- the Wright API.

SET LOCAL lock_timeout = '10s';

CREATE TABLE IF NOT EXISTS wright.followups (
    id             bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind           text        NOT NULL
                               CHECK (kind IN ('check_tuning', 'code_defect', 'hardening', 'root_cause')),
    check_name     text        REFERENCES wright.checks(name) ON DELETE SET NULL,
    owning_part    text,
    title          text        NOT NULL CHECK (btrim(title) <> '' AND char_length(title) <= 200),
    body           text        NOT NULL CHECK (btrim(body) <> ''),
    state          text        NOT NULL DEFAULT 'open'
                               CHECK (state IN ('open', 'filed', 'handed_off', 'fixed', 'dropped', 'stuck')),
    issue_number   integer     CHECK (issue_number > 0),
    issue_url      text,
    attempts       integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    fixer_session  uuid,
    fixer_machine  text,
    filed_at       timestamptz,
    handed_off_at  timestamptz,
    closed_at      timestamptz,
    closed_reason  text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT followups_issued_has_number
        CHECK (state NOT IN ('filed', 'handed_off', 'fixed') OR issue_number IS NOT NULL),
    CONSTRAINT followups_handed_off_has_fixer
        CHECK (state <> 'handed_off' OR (fixer_session IS NOT NULL AND fixer_machine IS NOT NULL)),
    CONSTRAINT followups_closed_has_time
        CHECK ((state IN ('fixed', 'dropped')) = (closed_at IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_wright_followups_one_live
    ON wright.followups (kind, COALESCE(check_name, ''), COALESCE(owning_part, ''))
    WHERE state IN ('open', 'filed', 'handed_off', 'stuck');
CREATE INDEX IF NOT EXISTS idx_wright_followups_state ON wright.followups (state, id);
CREATE INDEX IF NOT EXISTS idx_wright_followups_check
    ON wright.followups (check_name) WHERE check_name IS NOT NULL;

CREATE TABLE IF NOT EXISTS wright.followup_findings (
    followup_id bigint      NOT NULL REFERENCES wright.followups(id) ON DELETE CASCADE,
    finding_id  bigint      NOT NULL REFERENCES wright.findings(id) ON DELETE CASCADE,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (followup_id, finding_id)
);

CREATE INDEX IF NOT EXISTS idx_wright_followup_findings_finding
    ON wright.followup_findings (finding_id);

ALTER TABLE wright.investigations
    ADD COLUMN IF NOT EXISTS followup_id bigint REFERENCES wright.followups(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_wright_investigations_followup
    ON wright.investigations (followup_id) WHERE followup_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS wright.followup_limits (
    id                 boolean     PRIMARY KEY DEFAULT true CHECK (id),
    repo               text        NOT NULL DEFAULT 'temet-ai/mike' CHECK (btrim(repo) <> ''),
    daily_filed        integer     NOT NULL DEFAULT 3 CHECK (daily_filed BETWEEN 0 AND 20),
    daily_handoffs     integer     NOT NULL DEFAULT 2 CHECK (daily_handoffs BETWEEN 0 AND 10),
    max_attempts       integer     NOT NULL DEFAULT 2 CHECK (max_attempts BETWEEN 1 AND 5),
    recurrence_actions integer     NOT NULL DEFAULT 10 CHECK (recurrence_actions BETWEEN 2 AND 1000),
    recurrence_days    integer     NOT NULL DEFAULT 7 CHECK (recurrence_days BETWEEN 1 AND 30),
    fixers             jsonb       NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(fixers) = 'array'),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

INSERT INTO wright.followup_limits (id) VALUES (true) ON CONFLICT (id) DO NOTHING;

COMMENT ON TABLE wright.followups IS
    'Wright follow-up register: work an engineer run found, filed once as a GitHub issue and handed to a fixer (temet-ai/mike#269).';
COMMENT ON TABLE wright.followup_findings IS
    'The engineer findings that evidence a follow-up.';
COMMENT ON TABLE wright.followup_limits IS
    'Budgets for filing and hand-off, and the fixer machines; one row, changed only by the operator.';
