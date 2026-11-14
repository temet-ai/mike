-- mike-code: what the session said after it was told about a finding.
--
-- reply is the text of the session's answer on the request that delivered the
-- finding (the request that carried it, or for a push the session's next
-- captured request), copied whole by the outcome sweep a minute after
-- delivery. A finding the session kept usually says why here, which is how a
-- rule's false positives show up. NULL until the sweep has read it, or when
-- that answer had no text.

ALTER TABLE workspace.code_findings
    ADD COLUMN IF NOT EXISTS reply text,
    ADD COLUMN IF NOT EXISTS reply_at timestamptz;

CREATE INDEX IF NOT EXISTS idx_code_findings_reply_pending
    ON workspace.code_findings (delivered_at)
    WHERE delivered_request_id IS NOT NULL AND reply_at IS NULL;
