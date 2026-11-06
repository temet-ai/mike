-- MCP channel push-wake (issue #352): a second delivery surface for
-- mailbox.messages alongside next-request body injection (sent_request_id)
-- and the remote-control SSE wake. mike-mcp polls
-- POST /api/workspace/sessions/{id}/channel/claim every ~2s while a
-- development-channel-enabled Claude Code session is attached; a successful
-- claim stamps channel_delivered_at so the row drops out of every other
-- pending-delivery query and can never be body-injected or redelivered a
-- second time. Unlike sent_request_id this column has no FK to
-- requests.requests -- delivery here never happens inside a captured
-- request, so there is no request row to point at.

ALTER TABLE mailbox.messages
    ADD COLUMN channel_delivered_at timestamptz;

COMMENT ON COLUMN mailbox.messages.channel_delivered_at IS
    'Stamped by claim_channel_messages when mike-mcp''s channel poller atomically claims this row for delivery via notifications/claude/channel. A message with this set is delivered exactly like one with sent_request_id set -- ack_session_message accepts either -- but carries no captured request to point at, so sent_request_id stays NULL for channel-delivered rows.';

DROP INDEX IF EXISTS mailbox.idx_mailbox_messages_pending;
CREATE INDEX idx_mailbox_messages_pending ON mailbox.messages (to_session_id, id)
    WHERE sent_request_id IS NULL AND channel_delivered_at IS NULL;
