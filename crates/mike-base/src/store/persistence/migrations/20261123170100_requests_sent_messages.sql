-- One row per sent message whose content differs from the original message at
-- the same ord (or every sent message, when requests.requests.sent_message_count
-- is set). Bodies are stored in messages.messages in the same canonical,
-- cache_control-stripped form as requests.messages, reusing the row the request
-- or its predecessor already holds for that message.
CREATE TABLE IF NOT EXISTS requests.sent_messages (
    request_id    bigint  NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    ord           integer NOT NULL CHECK (ord >= 0),
    message_id    bigint  NOT NULL REFERENCES messages.messages(id),
    cache_control jsonb,
    PRIMARY KEY (request_id, ord)
);

CREATE INDEX IF NOT EXISTS idx_sent_messages_message_id
    ON requests.sent_messages (message_id);

COMMENT ON TABLE requests.sent_messages IS
    'Messages of the sent (upstream) body that differ from the original body''s message at the same ord. With requests.requests.sent_message_count set, every sent message has a row.';
COMMENT ON COLUMN requests.sent_messages.cache_control IS
    'The sent message''s cache_control placements, stripped before messages.messages hashes it; same shape as requests.messages.cache_control.';
