CREATE TABLE agents.work_item_attention (
    id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id             bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    kind                   text NOT NULL,
    subject                text NOT NULL,
    since                  timestamptz NOT NULL,
    message                text NOT NULL,
    first_seen_at          timestamptz NOT NULL DEFAULT now(),
    delivered_message_uuid uuid,
    CONSTRAINT uq_work_item_attention UNIQUE (session_id, kind, subject)
);

CREATE INDEX idx_work_item_attention_session_seen ON agents.work_item_attention (session_id, first_seen_at);
