-- Tag vocabulary + thread tagging. Tags attach to threads, not messages —
-- the thread is the unit an operator filters, mutes, or resolves.
CREATE TABLE mailbox.tags (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code       text NOT NULL UNIQUE,
    hue        smallint,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE mailbox.thread_tags (
    thread_id bigint NOT NULL REFERENCES mailbox.threads(id) ON DELETE CASCADE,
    tag_id    bigint NOT NULL REFERENCES mailbox.tags(id)    ON DELETE CASCADE,
    PRIMARY KEY (thread_id, tag_id)
);
CREATE INDEX idx_mailbox_thread_tags_tag ON mailbox.thread_tags (tag_id);

COMMENT ON TABLE mailbox.tags IS
    'Open vocabulary of thread tags (interned on demand: an unknown code on a send is inserted here, not rejected) — the seed rows below are the codes the field retrospectives already prove are needed.';

INSERT INTO mailbox.tags (code) VALUES
    ('handoff'), ('blocked'), ('review'), ('merge'),
    ('redeploy'), ('heads-up'), ('question'), ('decision')
ON CONFLICT (code) DO NOTHING;
