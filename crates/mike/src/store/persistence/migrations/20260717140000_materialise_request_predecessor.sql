-- Materialise the write-path's single authoritative predecessor + the
-- session-chronological ordinal onto requests.requests (turns thread
-- R1/R2/R9): computed once at capture, replacing the 3 per-read window
-- functions (LAG predecessor, ROW_NUMBER request_number x2) in
-- rehydrate.rs. Expand-only; existing rows backfilled by the follow-up
-- backfill run (artifacts/threads/turns/worklog.md).
ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS predecessor_request_id bigint
        REFERENCES requests.requests(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS request_number bigint;

-- Backs both the write-path's per-insert request_number COUNT and the
-- read-path's chronological ordering; (session_id, created_at, id)
-- covers the composite ORDER BY/WHERE used by both.
CREATE INDEX IF NOT EXISTS idx_requests_session_created_id
    ON requests.requests (session_id, created_at, id);
