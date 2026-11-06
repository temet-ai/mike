-- Structured context items attached to a mailbox message (a request id, a
-- commit sha, a URL, ...) — deep-linking pointers the recipient can jump to,
-- distinct from the free-text body. `kind` is a closed set (CHECK), revalidated
-- at the REST layer so an unknown kind 400s naming it rather than surfacing a
-- raw constraint violation.
CREATE TABLE mailbox.message_context_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_id bigint NOT NULL REFERENCES mailbox.messages(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('request','session','thread','message','commit','file','issue','url','note')),
    value text NOT NULL,
    label text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_mailbox_message_context_items_message ON mailbox.message_context_items (message_id);
