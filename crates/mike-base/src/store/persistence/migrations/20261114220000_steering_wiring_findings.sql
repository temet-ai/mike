-- steering.wiring_findings: the daily liveness report (temet-ai/mike#225).
--
-- The wiring worker (crates/mike-features/src/steerer/structural/wiring*.rs) runs a
-- few cheap checks every six hours, each looking for a feature that is wired but
-- doing nothing: a stage that never applies, a fleet event kind that is never
-- written, a toggle off for weeks, a steerer or shadow mode that was never
-- promoted, a scheduled job that has not succeeded, a closed fleet nobody audited,
-- memory items nobody judged. Each unhealthy result is one row keyed by
-- (check_kind, subject); the row stays open, with its figure refreshed on every
-- pass, until the check finds the subject healthy, which sets resolved_at. A subject
-- that turns unhealthy again reopens its row with a new first_seen.
--
-- figure_value is the number the check judged (a rate, a count, an age in hours);
-- figure_text is the sentence that carries it. figure_changed_at moves only when
-- figure_value moves (or the row reopens), so "stuck at the same figure since" is a
-- plain read of the row.

CREATE TABLE IF NOT EXISTS steering.wiring_findings (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    check_kind        text             NOT NULL CHECK (btrim(check_kind) <> ''),
    subject           text             NOT NULL CHECK (btrim(subject) <> ''),
    figure_value      double precision NOT NULL,
    figure_text       text             NOT NULL,
    first_seen        timestamptz      NOT NULL DEFAULT now(),
    last_seen         timestamptz      NOT NULL DEFAULT now(),
    figure_changed_at timestamptz      NOT NULL DEFAULT now(),
    resolved_at       timestamptz,
    CONSTRAINT wiring_findings_subject_unique UNIQUE (check_kind, subject)
);

CREATE INDEX IF NOT EXISTS idx_wiring_findings_open
    ON steering.wiring_findings (check_kind, subject) WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_wiring_findings_resolved_at
    ON steering.wiring_findings (resolved_at) WHERE resolved_at IS NOT NULL;

COMMENT ON TABLE steering.wiring_findings IS
    'One row per feature the liveness checks found wired but doing nothing, keyed by (check_kind, subject). Open while resolved_at is NULL; the figure is refreshed on every pass.';
COMMENT ON COLUMN steering.wiring_findings.check_kind IS
    'Which check: stage_applied, fleet_event_written, toggle_off, shadow_not_promoted, steerer_dormant, job_not_succeeding, closed_fleet_unaudited, supply_verdict_coverage, search_grade_coverage, lesson_retirement.';
COMMENT ON COLUMN steering.wiring_findings.subject IS
    'What the check judged, e.g. stage:recall_injector, toggle:db_resync, job:usage_audit.';
COMMENT ON COLUMN steering.wiring_findings.figure_value IS
    'The number the check judged: an applied or coverage rate in percent, a count, or an age in hours or days; the figure_text says which.';
COMMENT ON COLUMN steering.wiring_findings.figure_text IS
    'The figure as a sentence for the report, refreshed on every pass.';
COMMENT ON COLUMN steering.wiring_findings.first_seen IS
    'When the current run of unhealthy passes began; reset when a resolved row reopens.';
COMMENT ON COLUMN steering.wiring_findings.figure_changed_at IS
    'When figure_value last changed while the row was open, or when the row reopened.';
COMMENT ON COLUMN steering.wiring_findings.resolved_at IS
    'Set by the first pass whose check found the subject healthy; NULL while the finding is open.';
