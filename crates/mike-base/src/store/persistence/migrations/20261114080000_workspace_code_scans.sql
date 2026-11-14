-- mike-code: ast-grep scans of the files sessions change, and what each scan found.
--
-- One code_scans row per file change event the change record watcher wrote
-- (kind edit or shell_change, with an after blob): the language, the rule set
-- version, how many rules ran, the bytes and the time it took, the findings in
-- the file before and after the change, how many the change added and how many
-- it fixed, or why the file was skipped. Every scan is kept, including the ones
-- that found nothing, so the rules can be judged by how often they fire and
-- what sessions do next.
--
-- One code_findings row per finding a change added. The session is the change
-- event's session (named by the watcher a few seconds after the change), so it
-- is read through change_event_id rather than copied. delivered_request_id is
-- the request whose body carried the finding to the session; outcome moves from
-- open to fixed when a later scan of the same file no longer has it, or to
-- superseded when the file is deleted.

CREATE TABLE IF NOT EXISTS workspace.code_scans (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    change_event_id bigint NOT NULL REFERENCES workspace.change_events(id) ON DELETE CASCADE,
    checkout text NOT NULL,
    path text NOT NULL,
    scanned_at timestamptz NOT NULL DEFAULT now(),
    language text,
    rules_version text NOT NULL,
    rules_run integer NOT NULL,
    bytes integer NOT NULL,
    duration_us integer NOT NULL,
    skipped text,
    error text,
    findings_before integer NOT NULL,
    findings_after integer NOT NULL,
    findings_new integer NOT NULL,
    findings_fixed integer NOT NULL,
    CONSTRAINT code_scans_one_per_event UNIQUE (change_event_id)
);

CREATE INDEX IF NOT EXISTS idx_code_scans_checkout_id
    ON workspace.code_scans (checkout, id);
CREATE INDEX IF NOT EXISTS idx_code_scans_scanned_at
    ON workspace.code_scans (scanned_at);

CREATE TABLE IF NOT EXISTS workspace.code_findings (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    scan_id bigint NOT NULL REFERENCES workspace.code_scans(id) ON DELETE CASCADE,
    change_event_id bigint NOT NULL REFERENCES workspace.change_events(id) ON DELETE CASCADE,
    checkout text NOT NULL,
    path text NOT NULL,
    rule_id text NOT NULL,
    severity text NOT NULL,
    message text NOT NULL,
    line integer NOT NULL,
    col integer NOT NULL,
    matched text NOT NULL,
    fix text,
    fingerprint text NOT NULL,
    found_at timestamptz NOT NULL DEFAULT now(),
    delivered_request_id bigint,
    delivered_at timestamptz,
    outcome text NOT NULL DEFAULT 'open' CHECK (outcome IN ('open', 'fixed', 'superseded')),
    outcome_scan_id bigint REFERENCES workspace.code_scans(id) ON DELETE SET NULL,
    outcome_at timestamptz
);

CREATE INDEX IF NOT EXISTS idx_code_findings_open_path
    ON workspace.code_findings (checkout, path) WHERE outcome = 'open';
CREATE INDEX IF NOT EXISTS idx_code_findings_undelivered
    ON workspace.code_findings (change_event_id) WHERE delivered_request_id IS NULL AND outcome = 'open';
CREATE INDEX IF NOT EXISTS idx_code_findings_scan
    ON workspace.code_findings (scan_id);
CREATE INDEX IF NOT EXISTS idx_code_findings_found_at
    ON workspace.code_findings (found_at);

COMMENT ON TABLE workspace.code_scans IS
    'mike-code: one ast-grep scan per file change event, kept whether or not it found anything.';
COMMENT ON TABLE workspace.code_findings IS
    'mike-code: a finding a change added, its delivery to the session and its outcome.';
