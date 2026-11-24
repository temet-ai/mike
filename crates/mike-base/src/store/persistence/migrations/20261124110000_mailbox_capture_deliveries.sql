CREATE TABLE IF NOT EXISTS mailbox.capture_deliveries (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_id bigint NOT NULL REFERENCES mailbox.messages(id) ON DELETE CASCADE,
    capture_id bigint NOT NULL REFERENCES runtime.capture_exchanges(id) ON DELETE RESTRICT,
    delivered_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (message_id, capture_id)
);

CREATE INDEX IF NOT EXISTS idx_mailbox_capture_deliveries_capture
    ON mailbox.capture_deliveries (capture_id);
