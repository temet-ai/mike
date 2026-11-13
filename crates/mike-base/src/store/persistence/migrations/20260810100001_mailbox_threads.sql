CREATE TABLE mailbox.threads (
    id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    thread_uuid            uuid NOT NULL UNIQUE,
    subject                text NOT NULL,
    project_id             bigint REFERENCES projects.projects(id) ON DELETE SET NULL,
    created_by_session_id  bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    created_at             timestamptz NOT NULL DEFAULT now(),
    last_message_at        timestamptz NOT NULL DEFAULT now(),
    resolved_at            timestamptz
);

COMMENT ON COLUMN mailbox.threads.last_message_at IS
    'Bumped on every message appended to the thread (new send, reply, or retroactive adoption) — the sort key for the recent-threads views.';

-- Recent-threads-per-project listing (GET /api/threads?project=).
CREATE INDEX idx_mailbox_threads_project_recent ON mailbox.threads (project_id, last_message_at DESC);
-- Index every FK not already covered by the composite index above.
CREATE INDEX idx_mailbox_threads_created_by ON mailbox.threads (created_by_session_id) WHERE created_by_session_id IS NOT NULL;

ALTER TABLE mailbox.messages
    ADD COLUMN thread_id      bigint REFERENCES mailbox.threads(id) ON DELETE SET NULL,
    ADD COLUMN in_reply_to_id bigint REFERENCES mailbox.messages(id) ON DELETE SET NULL;

-- Oldest-first transcript fetch for one thread.
CREATE INDEX idx_mailbox_messages_thread ON mailbox.messages (thread_id, id) WHERE thread_id IS NOT NULL;
CREATE INDEX idx_mailbox_messages_in_reply_to ON mailbox.messages (in_reply_to_id) WHERE in_reply_to_id IS NOT NULL;
