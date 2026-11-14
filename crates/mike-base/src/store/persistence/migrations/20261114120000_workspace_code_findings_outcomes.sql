-- mike-code, second step: where a finding's rule came from, how it reached the
-- session, and two more outcomes.
--
-- rule_source is builtin for the rules compiled into mike and project for a
-- rule read from the checkout's own sgconfig.yml ruleDirs or .mike/code-rules/.
-- delivered_via is request when the finding was appended to a request body and
-- push when mike pushed it to an idle session over Remote Control; a pushed
-- finding's delivered_request_id is the next captured request of that session,
-- or NULL when none came.
-- outcome gains fixed_by_other (a later change by another session removed it)
-- and ignored (the session was told and the finding was still there when the
-- path was committed, or 30 minutes after it was told). An ignored finding a
-- later change removes still moves to fixed.

ALTER TABLE workspace.code_findings
    ADD COLUMN IF NOT EXISTS rule_source text NOT NULL DEFAULT 'builtin',
    ADD COLUMN IF NOT EXISTS delivered_via text;

ALTER TABLE workspace.code_findings
    DROP CONSTRAINT IF EXISTS code_findings_outcome_check,
    ADD CONSTRAINT code_findings_outcome_check
        CHECK (outcome IN ('open', 'fixed', 'fixed_by_other', 'ignored', 'superseded')),
    ADD CONSTRAINT code_findings_rule_source_check
        CHECK (rule_source IN ('builtin', 'project')),
    ADD CONSTRAINT code_findings_delivered_via_check
        CHECK (delivered_via IS NULL OR delivered_via IN ('request', 'push'));

DROP INDEX IF EXISTS workspace.idx_code_findings_open_path;
CREATE INDEX IF NOT EXISTS idx_code_findings_live_path
    ON workspace.code_findings (checkout, path) WHERE outcome IN ('open', 'ignored');
CREATE INDEX IF NOT EXISTS idx_code_findings_told_open
    ON workspace.code_findings (delivered_at) WHERE outcome = 'open' AND delivered_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_code_findings_fixed_recent
    ON workspace.code_findings (outcome_at) WHERE outcome = 'fixed';
