-- Envelope half of the sent-body reconstruction mechanism: bytes the proxy
-- changed OUTSIDE the `messages[]` array -- in practice the composer's
-- `system[2].text` splice. `requests.deltas`
-- (20260721010000_request_deltas_sent_patch.sql) carries the per-message
-- half on its "added" rows; `requests.deltas` has no row to hang a
-- non-message mutation on, so the envelope half lives here instead. Same
-- shape, same reconstruction fold (`messages::apply_sent_patches` treats
-- both sources identically once loaded).
--
-- Nullable, no default, metadata-only ADD COLUMN: every existing row reads
-- back as NULL unchanged, no backfill. NOT VALID for the same reason as the
-- sibling migration -- infra.local is disk-pressured, and every
-- pre-existing row is provably NULL (just-added column, no default), so a
-- VALIDATE scan buys no signal for real I/O cost. Left as an
-- operator-scheduled follow-up.
ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS sent_patch_start integer,
    ADD COLUMN IF NOT EXISTS sent_patch_end   integer,
    ADD COLUMN IF NOT EXISTS sent_patch_bytes bytea;

ALTER TABLE requests.requests
    ADD CONSTRAINT requests_sent_patch_all_or_none CHECK (
        (sent_patch_start IS NULL) = (sent_patch_end IS NULL)
        AND (sent_patch_end IS NULL) = (sent_patch_bytes IS NULL)
    ) NOT VALID,
    ADD CONSTRAINT requests_sent_patch_range CHECK (
        sent_patch_start IS NULL
        OR (sent_patch_start >= 0 AND sent_patch_end > sent_patch_start)
    ) NOT VALID;

COMMENT ON COLUMN requests.requests.sent_patch_start IS
    'Inclusive byte offset into request_body where the sent (upstream) form differs OUTSIDE the messages[] array (in practice, the composer system-block splice). NULL when nothing outside messages[] changed.';
COMMENT ON COLUMN requests.requests.sent_patch_end IS
    'Exclusive byte offset into request_body bounding the replaced envelope span. NULL when nothing outside messages[] changed.';
COMMENT ON COLUMN requests.requests.sent_patch_bytes IS
    'The raw bytes actually sent upstream for the envelope span, replacing request_body[sent_patch_start:sent_patch_end]. NULL when nothing outside messages[] changed.';
