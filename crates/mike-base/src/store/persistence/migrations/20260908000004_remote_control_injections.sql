-- Mike-authored wire events are separate from captured upstream traffic.
-- Reserve the mailbox claim and provenance row together, before emitting
-- bytes. A stream write is not a receipt: only the client's received ack
-- stamps acked_at and consumes the mailbox row.
CREATE TABLE remote_control.injections (
    event_id uuid PRIMARY KEY,
    channel_id bigint NOT NULL REFERENCES remote_control.channels(id) ON DELETE CASCADE,
    message_uuid uuid REFERENCES mailbox.messages(message_uuid) ON DELETE SET NULL,
    authored_by text NOT NULL DEFAULT 'mike' CHECK (authored_by = 'mike'),
    frame bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    emitted_at timestamptz,
    failed_at timestamptz,
    acked_at timestamptz,
    ack_call_id bigint REFERENCES remote_control.calls(id) ON DELETE SET NULL
);
CREATE INDEX idx_rc_injections_channel ON remote_control.injections(channel_id);
CREATE INDEX idx_rc_injections_message ON remote_control.injections(message_uuid)
    WHERE message_uuid IS NOT NULL;
CREATE INDEX idx_rc_injections_ack_call ON remote_control.injections(ack_call_id)
    WHERE ack_call_id IS NOT NULL;
COMMENT ON TABLE remote_control.injections IS
    'Exact mike-authored, id-less SSE frames, including unsuccessful attempts. '
    'emitted_at means handed to the reply stream; acked_at means the addressed '
    'client reported received, not proof that its model acted on the message.';
