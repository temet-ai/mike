-- Durable capture of unsuccessful `POST /api/sessions/{to}/message` attempts
-- (AC2, coordination-protocol-hardening thread). A wrong-recipient guess
-- previously left zero trace anywhere — a session's own comms-log claim of
-- "two failed 404 sends" was unverifiable from any table. This table makes
-- every failure branch of the mailbox POST handler durable and queryable.

CREATE TABLE mailbox.failed_sends (
    id                       bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    attempted_by_session_id  bigint      REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    attempted_to             text        NOT NULL,
    reason                   text        NOT NULL
                                 CHECK (reason IN (
                                     'empty_message',
                                     'invalid_from_session_id',
                                     'session_not_found',
                                     'from_session_not_found',
                                     'database_error'
                                 )),
    attempted_at             timestamptz NOT NULL DEFAULT now(),
    request_id               bigint      REFERENCES requests.requests(id) ON DELETE SET NULL
);

COMMENT ON COLUMN mailbox.failed_sends.attempted_to IS
    'Raw recipient string exactly as the caller sent it — may not resolve to any session; that is the point (a wrong-UUID guess is captured verbatim, not as a resolved FK).';
COMMENT ON COLUMN mailbox.failed_sends.reason IS
    'Mirrors the `error` taxonomy already returned by mailbox_error() in POST /api/sessions/{to}/message. persistence_disabled is deliberately excluded — that branch has no PersistHandle to write this row through.';
COMMENT ON COLUMN mailbox.failed_sends.request_id IS
    'Correlation to the inbound request driving this attempt; NULL today because a mailbox POST is not itself captured as a requests.requests row.';

-- Index every FK. Both are sparse today — attempted_by_session_id only when
-- the caller supplied a from_session_id that resolves, request_id not
-- populated by any current call site — so both are partial, matching
-- mailbox.messages' established idiom for the same shape of column.
CREATE INDEX idx_mailbox_failed_sends_attempted_by
    ON mailbox.failed_sends (attempted_by_session_id) WHERE attempted_by_session_id IS NOT NULL;
CREATE INDEX idx_mailbox_failed_sends_request
    ON mailbox.failed_sends (request_id) WHERE request_id IS NOT NULL;
-- Append-mostly time series — BRIN per repo convention for this workload shape.
CREATE INDEX brin_mailbox_failed_sends_attempted_at
    ON mailbox.failed_sends USING BRIN (attempted_at);
