-- Per-message mutation record for byte-exact `request_sent` reconstruction.
--
-- The proxy no longer stores a second full copy of the sent (upstream) body
-- (requests.requests.filtered_request_body is now always NULL for new rows --
-- see the file_read/filter/composer/injection pipeline in
-- crates/mike/src/proxy/handler.rs). Mutations average ~527 bytes against
-- bodies averaging ~1 MB, so a full second copy was a ~600x storage
-- amplifier. `request_sent` is reconstructed instead from
-- requests.requests.request_body plus a byte patch recorded on the message
-- the proxy actually changed.
--
-- Patches are hung on the EXISTING requests.deltas 'added' row for that
-- message (one row already exists per message new to this request's
-- lineage; see crates/mike/src/store/persistence/messages.rs). No second
-- full body anywhere: sent_patch_bytes carries only the mutated message's
-- own raw span, not the whole document. start/end are byte offsets into
-- requests.requests.request_body (half-open [start, end)); sent_patch_bytes
-- is the literal bytes that replace that span in the sent body. All three
-- NULL together means the proxy did not change that message.
--
-- Nullable, no default, metadata-only ADD COLUMN: every existing row reads
-- back as NULL unchanged, no backfill. CHECK constraints are added NOT
-- VALID so the ALTER never scans requests.deltas -- the new columns are
-- provably NULL on every pre-existing row (they were just added with no
-- default), so validating that is pure I/O against an already-tight disk
-- host for no signal; VALIDATE CONSTRAINT is left as an operator-scheduled
-- follow-up once that pressure is gone.
ALTER TABLE requests.deltas
    ADD COLUMN IF NOT EXISTS sent_patch_start integer,
    ADD COLUMN IF NOT EXISTS sent_patch_end   integer,
    ADD COLUMN IF NOT EXISTS sent_patch_bytes bytea;

ALTER TABLE requests.deltas
    ADD CONSTRAINT request_deltas_sent_patch_all_or_none CHECK (
        (sent_patch_start IS NULL) = (sent_patch_end IS NULL)
        AND (sent_patch_end IS NULL) = (sent_patch_bytes IS NULL)
    ) NOT VALID,
    ADD CONSTRAINT request_deltas_sent_patch_range CHECK (
        sent_patch_start IS NULL
        OR (sent_patch_start >= 0 AND sent_patch_end > sent_patch_start)
    ) NOT VALID,
    -- A 'removed' row records a message no longer present in this
    -- request's sent form either -- there is no span to patch.
    ADD CONSTRAINT request_deltas_sent_patch_only_on_added CHECK (
        sent_patch_start IS NULL OR change_kind_id = 1
    ) NOT VALID;

COMMENT ON COLUMN requests.deltas.sent_patch_start IS
    'Inclusive byte offset into requests.requests.request_body where the sent (upstream) form of this message differs. NULL when the proxy did not change this message.';
COMMENT ON COLUMN requests.deltas.sent_patch_end IS
    'Exclusive byte offset into requests.requests.request_body bounding the replaced span. NULL when the proxy did not change this message.';
COMMENT ON COLUMN requests.deltas.sent_patch_bytes IS
    'The raw bytes actually sent upstream for this message, replacing request_body[sent_patch_start:sent_patch_end]. NULL when the proxy did not change this message.';
