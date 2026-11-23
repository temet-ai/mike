-- The body mike forwarded upstream ("sent"), stored as content instead of as
-- byte-offset patches into request_body. Offsets into the raw body cannot be
-- applied once the body is rebuilt from its parts (messages re-serialised from
-- jsonb), and request_body is about to be cleared.
--
-- sent_envelope: set only when the sent body differs from the original outside
-- `messages`. It is the sent body with its top-level `messages` array emptied,
-- and its `tools` array emptied too when the sent tools are byte-identical to
-- the original's. An empty `tools` array is filled from requests.tools; tools
-- mike changed stay inline. NULL means the sent envelope is request_envelope.
--
-- sent_message_count: set when the sent body carries a different number of
-- messages than the original; requests.sent_messages then holds every sent
-- message. NULL means the sent messages are the original ones with the rows of
-- requests.sent_messages replacing theirs at the same ord.
--
-- Nullable, no default: metadata-only ADD COLUMN, no rewrite of
-- requests.requests. The sent_patch_* columns stay for rows that predate this
-- and cannot be backfilled.
ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS sent_envelope bytea,
    ADD COLUMN IF NOT EXISTS sent_message_count integer;

ALTER TABLE requests.requests
    DROP CONSTRAINT IF EXISTS requests_sent_message_count_nonneg;

ALTER TABLE requests.requests
    ADD CONSTRAINT requests_sent_message_count_nonneg
        CHECK (sent_message_count IS NULL OR sent_message_count >= 0) NOT VALID;

COMMENT ON COLUMN requests.requests.sent_envelope IS
    'The sent (upstream) body with its top-level messages array emptied, and its tools array emptied when the sent tools are byte-identical to the original tools. NULL when the sent body differs from the original only inside messages (or not at all).';
COMMENT ON COLUMN requests.requests.sent_message_count IS
    'Number of messages in the sent body when it differs from the original''s count; requests.sent_messages then has a row for every sent ord. NULL when the counts are equal.';
